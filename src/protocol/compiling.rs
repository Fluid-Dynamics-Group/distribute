use super::Either;
use super::Machine;
use crate::prelude::*;

pub(crate) struct Building;
pub(crate) struct ClientBuildingState {
    build_opt: transport::BuildOpts,
    conn: transport::Connection<ClientMsg>,
    working_dir: PathBuf,
}

pub(crate) struct ServerBuildingState {
    conn: transport::Connection<ServerMsg>,
    common: super::Common,
    namespace: String,
    batch_name: String,
    job_identifier: server::JobIdentifier,
}

// information on the next build state that we transition to
use super::built::{Built, ClientBuiltState, ServerBuiltState};
use super::prepare_build::{ClientPrepareBuildState, PrepareBuild, ServerPrepareBuildState};

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
    #[error("Failed compilation")]
    FailedCompilation,
}

impl Machine<Building, ClientBuildingState> {
    /// compile the job on the node and tell the server if the job was successfully built
    ///
    /// Successful compilations return `Machine<Built, _>`.
    ///
    /// Cancelled compilations or Failed compilations return `Machine<PrepareBuild, _>`
    ///
    /// this routine is also responsible for listening for cancellation requests from the server
    pub(crate) async fn build_job(
        mut self,
    ) -> Result<
        Either<Machine<Built, ClientBuiltState>, Machine<PrepareBuild, ClientPrepareBuildState>>,
        (Self, ClientError),
    > {
        // TODO: should probably wipe the folder and instantiate the folders here

        // TODO: monitor for cancellation
        let (tx_cancel, rx_cancel) = broadcast::channel(10);
        let (tx_result, rx_result) = oneshot::channel();

        let mut folder_state = crate::client::execute::BindingFolderState::new();
        let working_dir = self.state.working_dir.clone();
        // TODO: we can probably avoid this clone if we are clever with transporting the data
        // back from the proc after it finishes
        let build_opt = self.state.build_opt.clone();

        // spawn off a worker to perform the compilation so that we can monitor
        // for cancellation signals from the master node
        tokio::spawn(async move {
            let mut cancel = rx_cancel;

            match build_opt {
                transport::BuildOpts::Python(python_job) => {
                    let build_result =
                        crate::client::initialize_python_job(python_job, &working_dir, &mut cancel)
                            .await;
                    let msg = ClientMsg::from_build_result(build_result);
                    tx_result.send((folder_state, msg)).ok().unwrap();
                }
                transport::BuildOpts::Singularity(singularity_job) => {
                    let build_result = crate::client::initialize_singularity_job(
                        singularity_job,
                        &working_dir,
                        &mut cancel,
                        &mut folder_state,
                    )
                    .await;
                    let msg = ClientMsg::from_build_result(build_result);
                    tx_result.send((folder_state, msg)).ok().unwrap();
                }
            }
        });

        let (folder_state, msg) = rx_result.await.unwrap();

        // tell the node about what the result was
        throw_error_with_self!(self.state.conn.transport_data(&msg).await, self);

        match msg {
            ClientMsg::SuccessfullCompilation => {
                // go to Machine<Built, _>
            }
            ClientMsg::FailedCompilation | ClientMsg::CancelledCompilation => {
                // go to Machine<PrepareBuild, _>
            }
        }

        // TODO: pass on folder state to the next state

        todo!()
    }

    pub(crate) fn to_uninit(self) -> super::UninitClient {
        todo!()
    }
}

impl Machine<Building, ServerBuildingState> {
    /// wait for the node to compile the job and tell us about the result
    ///
    /// Successful compilations return `Machine<Built, _>`.
    ///
    /// Cancelled compilations or Failed compilations return `Machine<PrepareBuild, _>`
    ///
    /// this routine is also responsible for listening for cancellation requests from the server
    pub(crate) async fn prepare_for_execution(
        mut self,
    ) -> Result<
        Either<Machine<Built, ServerBuiltState>, Machine<PrepareBuild, ServerPrepareBuildState>>,
        ServerError,
    > {
        let msg = self.state.conn.receive_data().await?;

        let out = match msg {
            ClientMsg::SuccessfullCompilation => {
                // TODO: go to Built state
                todo!()
            }
            ClientMsg::FailedCompilation => {
                self.state
                    .common
                    .errored_jobs
                    .insert(self.state.job_identifier);
                // TODO: Return to Machine<PrepareBuild, _> here
                todo!()
            }
            ClientMsg::CancelledCompilation => {
                // we dont need to mark the job as cancelled since the job
                // should no longer exist in the system (it was cancelled everywhere)

                // TODO: return to Machine<PrepareBuild, _> here
                todo!()
            }
        };

        Ok(out)
    }

    pub(crate) fn to_uninit(self) -> super::UninitServer {
        todo!()
    }
}

#[derive(Serialize, Deserialize, Unwrap)]
pub(crate) enum ServerMsg {}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub(crate) enum ClientMsg {
    SuccessfullCompilation,
    FailedCompilation,
    CancelledCompilation,
}

impl ClientMsg {
    fn from_build_result(result: Result<Option<()>, error::Error>) -> Self {
        // check what the build result was
        match result {
            Ok(None) => {
                // The build process was cancelled since the job was cancelled
                Self::CancelledCompilation
            }
            Ok(Some(_)) => {
                // the process built perfectly
                Self::SuccessfullCompilation
            }
            Err(e) => {
                // the job failed to build
                Self::FailedCompilation
            }
        }
    }
}

impl transport::AssociatedMessage for ServerMsg {
    type Receive = ClientMsg;
}

impl transport::AssociatedMessage for ClientMsg {
    type Receive = ServerMsg;
}