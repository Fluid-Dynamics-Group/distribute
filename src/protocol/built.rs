use super::Either;
use super::Machine;
use crate::prelude::*;

use super::executing::{ClientExecutingState, Executing, ServerExecutingState};

#[derive(thiserror::Error, Debug, From)]
pub(crate) enum ClientError {
    #[error("{0}")]
    TcpConnection(error::TcpConnection),
}

#[derive(thiserror::Error, Debug, From)]
pub(crate) enum ServerError {
    #[error("{0}")]
    TcpConnection(error::TcpConnection),
    #[error("Node failed the keepalive check")]
    MissedKeepalive,
}

#[derive(Default)]
pub(crate) struct Built;

pub(crate) struct ClientBuiltState {
    pub(super) conn: transport::Connection<ClientMsg>,
    pub(super) working_dir: PathBuf,
    pub(super) folder_state: client::BindingFolderState,
}

pub(crate) struct ServerBuiltState {
    pub(super) conn: transport::Connection<ServerMsg>,
    pub(super) common: super::Common,
    pub(super) namespace: String,
    pub(super) batch_name: String,
    // the job identifier we have scheduled to run
    pub(super) job_identifier: server::JobIdentifier,
}

impl Machine<Built, ClientBuiltState> {
    /// wait for the node to return information on the job we are to run
    pub(crate) async fn get_execute_instructions(
        mut self,
    ) -> Result<
        super::ClientEitherPrepareBuild<Machine<Executing, ClientExecutingState>>,
        (Self, ClientError),
    > {
        let msg = self.state.conn.receive_data().await;
        let msg: ServerMsg = throw_error_with_self!(msg, self);

        match msg {
            ServerMsg::ExecuteJob(job) => {
                // return Machine<Executing, _>
                let executing_state = self.into_executing_state(job).await;
                let machine = Machine::from_state(executing_state);
                Ok(Either::Right(machine))
            }
            ServerMsg::ReturnPrepareBuild => {
                // return Machine<PrepareBuild, _>
                let prepare_build_state = self.into_prepare_build_state().await;
                let machine = Machine::from_state(prepare_build_state);
                Ok(Either::Left(machine))
            }
        }
    }

    pub(crate) fn to_uninit(self) -> super::UninitClient {
        let ClientBuiltState {
            conn, working_dir, ..
        } = self.state;
        let conn = conn.update_state();
        debug!("moving client built -> uninit");
        let state = super::uninit::ClientUninitState { conn, working_dir };
        Machine::from_state(state)
    }

    async fn into_prepare_build_state(self) -> super::prepare_build::ClientPrepareBuildState {
        let ClientBuiltState {
            conn, working_dir, ..
        } = self.state;
        debug!("moving client built -> prepare build");
        #[allow(unused_mut)]
        let mut conn = conn.update_state();

        #[cfg(test)]
        assert!(conn.bytes_left().await == 0);
        super::prepare_build::ClientPrepareBuildState { conn, working_dir }
    }

    async fn into_executing_state(
        self,
        job: transport::JobOpt,
    ) -> super::executing::ClientExecutingState {
        let ClientBuiltState {
            conn,
            working_dir,
            folder_state,
        } = self.state;
        debug!("moving client built -> executing");

        #[allow(unused_mut)]
        let mut conn = conn.update_state();

        #[cfg(test)]
        assert!(conn.bytes_left().await == 0);

        super::executing::ClientExecutingState {
            conn,
            working_dir,
            job,
            folder_state,
        }
    }
}

impl Machine<Built, ServerBuiltState> {
    /// fetch job details form the scheduler and inform the compute node of the data that is
    /// required to build the job
    pub(crate) async fn send_job_execution_instructions(
        mut self,
        scheduler_tx: &mut mpsc::Sender<server::JobRequest>,
    ) -> Result<
        super::ServerEitherPrepareBuild<Machine<Executing, ServerExecutingState>>,
        (Self, ServerError),
    > {
        let job = server::node::fetch_new_job(
            scheduler_tx,
            self.state.job_identifier,
            &self.state.common.node_name,
            &self.state.common.main_transport_addr,
            &self.state.common.keepalive_addr,
            self.state.common.capabilities.clone(),
            self.state.common.errored_jobs.clone(),
        )
        .await;

        // TODO: specify the query function that we only receive BuildTaskInfo
        //       and then we wont have the possibility of erroring here
        //let build_job: server::pool_data::BuildTaskInfo =
        match job {
            server::pool_data::FetchedJob::Build(build) => {
                //
                // we need to compile a different job and transition to the PrepareBuild state
                //
                if build.identifier == self.state.job_identifier {
                    error!("scheduler returned a build instruction for a job we have already compiled on {} / {} This is a bug", self.state.common.node_name, self.state.common.main_transport_addr);
                    panic!("scheduler returned a build instruction for a job we have already compiled on {} / {} This is a bug", self.state.common.node_name, self.state.common.main_transport_addr);
                } else {
                    // notify the compute machine that we are transitioning states
                    let tmp = self
                        .state
                        .conn
                        .transport_data(&ServerMsg::ReturnPrepareBuild)
                        .await;

                    throw_error_with_self!(tmp, self);

                    // return a Machine<PrepareBuild, _> since the scheudler wants us to
                    // prepare and run a different job
                    let prepare_build_state = self.into_prepare_build_state().await;
                    let machine = Machine::from_state(prepare_build_state);
                    Ok(Either::Left(machine))
                }
            }
            server::pool_data::FetchedJob::Run(run) => {
                //
                // We have been assigned to run a job that we have already compiled
                //

                // TODO: add some logs about what job we have been assigned to run

                let job_name = run.task.name().to_string();

                let tmp = self
                    .state
                    .conn
                    .transport_data(&ServerMsg::ExecuteJob(run.task))
                    .await;

                throw_error_with_self!(tmp, self);

                // return a Machine<Executing, _>
                let executing_state = self.into_executing_state(job_name).await;
                let machine = Machine::from_state(executing_state);
                Ok(Either::Right(machine))
            }
            // missed the keepalive, we should error out and let the caller handle this
            //
            // this can happen because we always check the node keepalive address before
            // we fetch new jobss in `fetch_new_job()`
            server::pool_data::FetchedJob::MissedKeepalive => {
                Err((self, ServerError::MissedKeepalive))
            }
        }
    }

    pub(crate) fn to_uninit(self) -> super::UninitServer {
        let ServerBuiltState { conn, common, .. } = self.state;
        let conn = conn.update_state();
        debug!("moving server built -> uninit");
        let state = super::uninit::ServerUninitState { conn, common };
        Machine::from_state(state)
    }

    async fn into_prepare_build_state(self) -> super::prepare_build::ServerPrepareBuildState {
        debug!("moving server built -> prepare_build");
        let ServerBuiltState { conn, common, .. } = self.state;

        #[allow(unused_mut)]
        let mut conn = conn.update_state();

        #[cfg(test)]
        assert!(conn.bytes_left().await == 0);

        super::prepare_build::ServerPrepareBuildState { conn, common }
    }

    async fn into_executing_state(
        self,
        job_name: String,
    ) -> super::executing::ServerExecutingState {
        debug!("moving server built -> executing");

        let ServerBuiltState {
            conn,
            common,
            namespace,
            batch_name,
            job_identifier,
        } = self.state;

        #[allow(unused_mut)]
        let mut conn = conn.update_state();

        #[cfg(test)]
        assert!(conn.bytes_left().await == 0);

        let save_location = common
            .save_path
            .join(&namespace)
            .join(&batch_name)
            .join(&job_name);

        super::executing::ServerExecutingState {
            conn,
            common,
            namespace,
            batch_name,
            job_identifier,
            job_name,
            save_location,
        }
    }
}

#[derive(Serialize, Deserialize, Unwrap)]
pub(crate) enum ServerMsg {
    ReturnPrepareBuild,
    ExecuteJob(transport::JobOpt),
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub(crate) enum ClientMsg {}

impl transport::AssociatedMessage for ServerMsg {
    type Receive = ClientMsg;
}

impl transport::AssociatedMessage for ClientMsg {
    type Receive = ServerMsg;
}
