use super::Either;
use super::Machine;
use crate::prelude::*;

use super::executing::{ClientExecutingState, Executing, ServerExecutingState};

#[derive(thiserror::Error, Debug, From)]
pub(crate) enum Error {
    #[error("{0}")]
    TcpConnection(error::TcpConnection),
    #[error("Node failed the keepalive check")]
    MissedKeepalive,
}

pub(crate) struct Built;
pub(crate) struct ClientBuiltState {
    conn: transport::Connection<ClientMsg>,
    working_dir: PathBuf,
    folder_state: client::BindingFolderState,
}

pub(crate) struct ServerBuiltState {
    conn: transport::Connection<ServerMsg>,
    common: super::Common,
    namespace: String,
    batch_name: String,
    // the job identifier we have scheduled to run
    job_identifier: server::JobIdentifier,
}

impl Machine<Built, ClientBuiltState> {
    /// wait for the node to return information on the job we are to run
    pub(crate) async fn get_execute_instructions(
        mut self,
    ) -> Result<super::ClientEitherPrepareBuild<Machine<Executing, ClientExecutingState>>, Error>
    {
        let msg = self.state.conn.receive_data().await?;

        match msg {
            ServerMsg::ExecuteJob(job) => {
                // TODO: return Machine<Executing, _>
            }
            ServerMsg::ReturnPrepareBuild => {
                // TODO: return Machine<PrepareBuild, _>
            }
        };

        todo!()
    }
}

impl Machine<Built, ServerBuiltState> {
    /// fetch job details form the scheduler and inform the compute node of the data that is
    /// required to build the job
    pub(crate) async fn send_job_execution_instructions(
        mut self,
        scheduler_tx: &mut mpsc::Sender<server::JobRequest>,
    ) -> Result<super::ServerEitherPrepareBuild<Machine<Executing, ServerExecutingState>>, Error>
    {
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
        let build_job: server::pool_data::BuildTaskInfo = match job {
            server::pool_data::FetchedJob::Build(build) => {
                if build.identifier == self.state.job_identifier {
                    error!("scheduler returned a build instruction for a job we have already compiled on {} / {} This is a bug", self.state.common.node_name, self.state.common.main_transport_addr);
                    panic!("scheduler returned a build instruction for a job we have already compiled on {} / {} This is a bug", self.state.common.node_name, self.state.common.main_transport_addr);
                } else {
                    // notify the compute machine that we are transitioning states
                    self.state
                        .conn
                        .transport_data(&ServerMsg::ReturnPrepareBuild)
                        .await?;

                    // TODO: return a Machine<PrepareBuild, _> since the scheudler wants us to
                    // prepare and run a different job
                    todo!()
                }
            }
            server::pool_data::FetchedJob::Run(run) => {
                self.state
                    .conn
                    .transport_data(&ServerMsg::ExecuteJob(run.task))
                    .await?;
                // TODO: return a Machine<Executing, _>
                todo!()
            }
            // missed the keepalive, we should error out and let the caller handle this
            server::pool_data::FetchedJob::MissedKeepalive => return Err(Error::MissedKeepalive),
        };

        todo!()
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
