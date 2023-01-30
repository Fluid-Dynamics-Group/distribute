use super::Either;
use super::Machine;
use crate::prelude::*;

use super::send_files::{self, SendFiles};

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

#[derive(Default, Debug)]
pub(crate) struct Built;

#[derive(Debug)]
pub(crate) struct ClientBuiltState {
    pub(super) conn: transport::Connection<ClientMsg>,
    pub(super) working_dir: WorkingDir,
    pub(super) folder_state: client::BindingFolderState,
    pub(super) cancel_addr: SocketAddr,
}

#[derive(Debug)]
pub(crate) struct ServerBuiltState {
    pub(super) conn: transport::Connection<ServerMsg>,
    pub(super) common: super::Common,
    pub(super) namespace: String,
    pub(super) batch_name: String,
    // the job identifier we have scheduled to run
    pub(super) job_identifier: server::JobSetIdentifier,
}

type ReceiveState = send_files::ReceiverState<send_files::ExecutingReceiver>;
type SendState = send_files::SenderState<send_files::ExecutingSender>;

impl Machine<Built, ClientBuiltState> {
    /// wait for the node to return information on the job we are to run
    #[instrument(skip(self), fields(working_dir = %self.state.working_dir))]
    pub(crate) async fn get_execute_instructions(
        mut self,
    ) -> Result<
        super::ClientEitherPrepareBuild<Machine<SendFiles, ReceiveState>>,
        (Self, ClientError),
    > {
        info!("now in built state");

        if let Err(e) = self.state.working_dir.clean_distribute_save().await {
            error!(
                "could not clean distribute save located inside {}, error: {e}",
                self.state.working_dir
            );
            #[cfg(test)]
            panic!(
                "could not clean distribute save located inside {}, error: {e}",
                self.state.working_dir
            );
        }

        if let Err(e) = self.state.working_dir.clean_input().await {
            error!(
                "could not clean input files located inside {}, error: {e}",
                self.state.working_dir
            );
            #[cfg(test)]
            panic!(
                "could not clean input files located inside {}, error: {e}",
                self.state.working_dir
            );
        }

        let msg = self.state.conn.receive_data().await;
        let msg: ServerMsg = throw_error_with_self!(msg, self);

        match msg {
            ServerMsg::ExecuteJob(job) => {
                // return Machine<Executing, _>

                debug!(
                    "client got executing instructions from the server for job name {}",
                    job.task.name()
                );

                let receive_files = self.into_receive_files_state(job).await;
                let machine = Machine::from_state(receive_files);
                Ok(Either::Right(machine))
            }
            ServerMsg::ReturnPrepareBuild => {
                // return Machine<PrepareBuild, _>

                debug!("got build instructions from the server");

                let prepare_build_state = self.into_prepare_build_state().await;
                let machine = Machine::from_state(prepare_build_state);
                Ok(Either::Left(machine))
            }
        }
    }

    pub(crate) fn into_uninit(self) -> super::UninitClient {
        let ClientBuiltState {
            conn,
            working_dir,
            cancel_addr,
            ..
        } = self.state;
        let conn = conn.update_state();
        debug!("moving client built -> uninit");
        let state = super::uninit::ClientUninitState {
            conn,
            working_dir,
            cancel_addr,
        };
        Machine::from_state(state)
    }

    async fn into_prepare_build_state(self) -> super::prepare_build::ClientPrepareBuildState {
        let ClientBuiltState {
            conn,
            working_dir,
            cancel_addr,
            ..
        } = self.state;
        debug!("moving client built -> prepare build");
        #[allow(unused_mut)]
        let mut conn = conn.update_state();

        #[cfg(test)]
        assert!(conn.bytes_left().await == 0);
        super::prepare_build::ClientPrepareBuildState {
            conn,
            working_dir,
            cancel_addr,
        }
    }

    async fn into_receive_files_state(
        self,
        run_info: server::pool_data::RunTaskInfo,
    ) -> ReceiveState {
        let ClientBuiltState {
            conn,
            working_dir,
            folder_state,
            cancel_addr,
        } = self.state;
        debug!("moving client built -> executing");

        #[allow(unused_mut)]
        let mut conn = conn.update_state();

        #[cfg(test)]
        assert!(conn.bytes_left().await == 0);

        // dump all the files in the ./input directory
        let save_location = working_dir.input_folder();

        let extra = send_files::ExecutingReceiver {
            working_dir,
            run_info,
            folder_state,
            cancel_addr,
            node_meta: server::pool_data::NodeMetadata::new(
                "SERVER".into(),
                ([0, 0, 0, 0], 0).into(),
            ),
        };

        ReceiveState {
            conn,
            save_location,
            extra,
        }
    }
}

impl Machine<Built, ServerBuiltState> {
    /// fetch job details form the scheduler and inform the compute node of the data that is
    /// required to build the job
    #[instrument(
        skip(self, scheduler_tx), 
        fields(
            node_meta = %self.state.common.node_meta,
            namespace = self.state.namespace,
            batch_name = self.state.batch_name,
        )
    )]
    pub(crate) async fn send_job_execution_instructions(
        mut self,
        scheduler_tx: &mut mpsc::Sender<server::JobRequest>,
    ) -> Result<super::ServerEitherPrepareBuild<Machine<SendFiles, SendState>>, (Self, ServerError)>
    {
        info!("{} now in built state", self.state.common.node_meta);

        let job = server::node::fetch_new_job(
            scheduler_tx,
            self.state.job_identifier,
            &self.state.common.node_meta,
            &self.state.common.keepalive_addr,
            self.state.common.capabilities.clone(),
            self.state.common.errored_jobs.clone(),
        )
        .await;

        match job {
            server::pool_data::FetchedJob::Build(build) => {
                //
                // we need to compile a different job and transition to the PrepareBuild state
                //

                debug!(
                    "{} got build instructions from the job pool",
                    self.state.common.node_meta
                );
                if build.identifier == self.state.job_identifier {
                    error!("scheduler returned a build instruction for a job we have already compiled on {} This is a bug", self.state.common.node_meta);
                    panic!("scheduler returned a build instruction for a job we have already compiled on {} This is a bug", self.state.common.node_meta);
                } else {
                    debug!(
                        "{} notifying compute node to transition states to prepare build",
                        self.state.common.node_meta
                    );

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

                debug!(
                    "{} got execute instructions from the job pool, job name is {}",
                    self.state.common.node_meta,
                    run.task.name()
                );

                let tmp = self
                    .state
                    .conn
                    .transport_data(&ServerMsg::ExecuteJob(run.clone()))
                    .await;

                throw_error_with_self!(tmp, self);

                // return a Machine<SendFiles, _>
                let send_state = self.into_send_files_state(run).await;
                let machine = Machine::from_state(send_state);
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

    pub(crate) fn into_uninit(self) -> super::UninitServer {
        let ServerBuiltState { conn, common, .. } = self.state;
        let conn = conn.update_state();
        debug!("moving {} server built -> uninit", common.node_meta);
        let state = super::uninit::ServerUninitState { conn, common };
        Machine::from_state(state)
    }

    async fn into_prepare_build_state(self) -> super::prepare_build::ServerPrepareBuildState {
        debug!(
            "moving {} server built -> prepare_build",
            self.state.common.node_meta
        );
        let ServerBuiltState { conn, common, .. } = self.state;

        #[allow(unused_mut)]
        let mut conn = conn.update_state();

        #[cfg(test)]
        assert!(conn.bytes_left().await == 0);

        super::prepare_build::ServerPrepareBuildState { conn, common }
    }

    async fn into_send_files_state(
        self,
        run_info: server::pool_data::RunTaskInfo,
    ) -> send_files::SenderState<send_files::ExecutingSender> {
        debug!(
            "{} is moving built -> executing",
            self.state.common.node_meta
        );

        let ServerBuiltState {
            conn,
            common,
            namespace,
            batch_name,
            job_identifier: _,
        } = self.state;

        #[allow(unused_mut)]
        let mut conn = conn.update_state();

        #[cfg(test)]
        assert!(conn.bytes_left().await == 0);

        let job_name = run_info.task.name();

        let save_location = common
            .save_path
            .join(namespace)
            .join(batch_name)
            .join(&job_name);

        let extra = send_files::ExecutingSender {
            common,
            run_info,
            save_location,
        };

        send_files::SenderState { conn, extra }
    }
}

#[derive(Serialize, Deserialize, Unwrap)]
pub(crate) enum ServerMsg {
    ReturnPrepareBuild,
    ExecuteJob(server::pool_data::RunTaskInfo),
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub(crate) enum ClientMsg {}

impl transport::AssociatedMessage for ServerMsg {
    type Receive = ClientMsg;
}

impl transport::AssociatedMessage for ClientMsg {
    type Receive = ServerMsg;
}
