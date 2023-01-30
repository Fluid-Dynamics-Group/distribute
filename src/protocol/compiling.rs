//! Compile a job so that we can execute multiple jobs from it
//!
//! These functions do not attempt to receive cancellation signals
//! since any compilation will simply receive no jobs later, at the cost
//! of possibly some minimal computational time lost
use super::Either;
use super::Machine;
use crate::prelude::*;

#[derive(Default)]
pub(crate) struct Building;

pub(crate) struct ClientBuildingState {
    pub(crate) build_info: server::pool_data::BuildTaskInfo,
    pub(super) conn: transport::Connection<ClientMsg>,
    pub(super) working_dir: WorkingDir,
    pub(super) cancel_addr: SocketAddr,
}

pub(crate) struct ServerBuildingState {
    pub(super) conn: transport::Connection<ServerMsg>,
    pub(super) common: super::Common,
    pub(super) namespace: String,
    pub(super) batch_name: String,
    pub(super) job_identifier: server::JobSetIdentifier,
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
    #[error("Unknown Client Error")]
    ClientError,
}

impl Machine<Building, ClientBuildingState> {
    /// compile the job on the node and tell the server if the job was successfully built
    ///
    /// Successful compilations return `Machine<Built, _>`.
    ///
    /// Cancelled compilations or Failed compilations return `Machine<PrepareBuild, _>`
    ///
    /// this routine is also responsible for listening for cancellation requests from the server
    #[instrument(skip(self), fields(batch_name=self.state.build_info.batch_name))]
    pub(crate) async fn build_job(
        mut self,
    ) -> Result<
        Either<Machine<Built, ClientBuiltState>, Machine<PrepareBuild, ClientPrepareBuildState>>,
        (Self, ClientError),
    > {
        // TODO: monitor for cancellation
        let (tx_result, rx_result) = oneshot::channel();

        let mut folder_state = crate::client::execute::BindingFolderState::new();
        let working_dir = self.state.working_dir.clone();
        // TODO: we can probably avoid this clone if we are clever with transporting the data
        // back from the proc after it finishes
        let build_info = self.state.build_info.clone();

        // spawn off a worker to perform the compilation so that we can monitor
        // for cancellation signals from the master node
        tokio::spawn(async move {
            match build_info.init {
                config::Init::Python(python_init) => {
                    let build_result = crate::client::initialize_python_job(
                        &python_init,
                        &build_info.batch_name,
                        &working_dir,
                    )
                    .await;
                    let msg = ClientMsg::from_build_result(build_result);
                    tx_result.send((folder_state, msg)).ok().unwrap();
                }
                config::Init::Apptainer(apptainer_init) => {
                    let build_result = crate::client::initialize_apptainer_job(
                        &apptainer_init,
                        &working_dir,
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
                let built_state = self.into_built_state(folder_state).await;
                let machine = Machine::from_state(built_state);
                Ok(Either::Left(machine))
            }
            ClientMsg::FailedCompilation => {
                // go to Machine<PrepareBuild, _>
                let prepare_build = self.into_prepare_build().await;
                let machine = Machine::from_state(prepare_build);
                Ok(Either::Right(machine))
            }
        }
    }

    pub(crate) fn into_uninit(self) -> super::UninitClient {
        let ClientBuildingState {
            conn,
            working_dir,
            cancel_addr,
            ..
        } = self.state;
        let conn = conn.update_state();
        let state = super::uninit::ClientUninitState {
            conn,
            working_dir,
            cancel_addr,
        };
        debug!("moving client compiling -> uninit");
        Machine::from_state(state)
    }

    pub(crate) async fn into_built_state(
        self,
        folder_state: client::BindingFolderState,
    ) -> super::built::ClientBuiltState {
        debug!("moving client compiling -> built");
        let ClientBuildingState {
            conn,
            working_dir,
            cancel_addr,
            ..
        } = self.state;

        #[allow(unused_mut)]
        let mut conn = conn.update_state();

        #[cfg(test)]
        assert!(conn.bytes_left().await == 0);

        super::built::ClientBuiltState {
            conn,
            working_dir,
            folder_state,
            cancel_addr,
        }
    }

    pub(crate) async fn into_prepare_build(self) -> super::prepare_build::ClientPrepareBuildState {
        let ClientBuildingState {
            conn,
            working_dir,
            cancel_addr,
            ..
        } = self.state;

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
}

impl Machine<Building, ServerBuildingState> {
    /// wait for the node to compile the job and tell us about the result
    ///
    /// Successful compilations return `Machine<Built, _>`.
    ///
    /// Cancelled compilations or Failed compilations return `Machine<PrepareBuild, _>`
    ///
    /// this routine is also responsible for listening for cancellation requests from the server
    #[instrument(
        skip(self), 
        fields(
            node_meta = %self.state.common.node_meta,
            namespace = self.state.namespace,
            batch_name = self.state.batch_name,
        )
    )]
    pub(crate) async fn prepare_for_execution(
        mut self,
    ) -> Result<
        Either<Machine<Built, ServerBuiltState>, Machine<PrepareBuild, ServerPrepareBuildState>>,
        (Self, ServerError),
    > {
        let tmp = self.state.conn.receive_data().await;

        let msg = throw_error_with_self!(tmp, self);

        let out = match msg {
            ClientMsg::SuccessfullCompilation => {
                // go to Built state
                let built_state = self.into_built_state().await;
                let machine = Machine::from_state(built_state);
                Either::Left(machine)
            }
            ClientMsg::FailedCompilation => {
                // mark the job as unbuildable and then
                // Return to Machine<PrepareBuild, _>
                self.state
                    .common
                    .errored_jobs
                    .insert(self.state.job_identifier);
                let prepare_build = self.into_prepare_build().await;
                let machine = Machine::from_state(prepare_build);
                Either::Right(machine)
            }
        };

        Ok(out)
    }

    pub(crate) fn into_uninit(self) -> super::UninitServer {
        let ServerBuildingState { conn, common, .. } = self.state;
        let conn = conn.update_state();
        let state = super::uninit::ServerUninitState { conn, common };
        debug!(
            "moving {} server compiling -> uninit",
            state.common.node_meta
        );
        Machine::from_state(state)
    }

    pub(crate) async fn into_built_state(self) -> super::built::ServerBuiltState {
        debug!(
            "moving {} server compiling -> built for job set name {} (ident: {})",
            self.state.common.node_meta, self.state.batch_name, self.state.job_identifier,
        );

        let ServerBuildingState {
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

        super::built::ServerBuiltState {
            conn,
            common,
            namespace,
            batch_name,
            job_identifier,
        }
    }

    pub(crate) async fn into_prepare_build(self) -> super::prepare_build::ServerPrepareBuildState {
        debug!(
            "moving {} server compiling -> prepare build",
            self.state.common.node_meta
        );

        let ServerBuildingState { conn, common, .. } = self.state;

        #[allow(unused_mut)]
        let mut conn = conn.update_state();

        #[cfg(test)]
        assert!(conn.bytes_left().await == 0);
        super::prepare_build::ServerPrepareBuildState { conn, common }
    }
}

#[derive(Serialize, Deserialize, Unwrap, Debug)]
pub(crate) enum ServerMsg {}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub(crate) enum ClientMsg {
    SuccessfullCompilation,
    FailedCompilation,
}

impl ClientMsg {
    fn from_build_result(result: Result<(), error::Error>) -> Self {
        // check what the build result was
        match result {
            Ok(_) => {
                // the process built perfectly
                Self::SuccessfullCompilation
            }
            Err(_) => {
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
