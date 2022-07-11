use super::Either;
use super::Machine;
use crate::prelude::*;

use super::send_files::{ClientSendFilesState, SendFiles, ServerSendFilesState};

#[derive(Default)]
pub(crate) struct Executing;

pub(crate) struct ClientExecutingState {
    pub(super) conn: transport::Connection<ClientMsg>,
    pub(super) working_dir: PathBuf,
    pub(super) job: transport::JobOpt,
    pub(super) folder_state: client::BindingFolderState,
}

pub(crate) struct ServerExecutingState {
    pub(super) conn: transport::Connection<ServerMsg>,
    pub(super) common: super::Common,
    pub(super) namespace: String,
    pub(super) batch_name: String,
    // the job identifier we have scheduled to run
    pub(super) job_identifier: server::JobIdentifier,
    pub(super) job_name: String,
    pub(super) save_location: PathBuf,
}

#[derive(thiserror::Error, Debug, From)]
pub(crate) enum ClientError {
    #[error("{0}")]
    TcpConnection(error::TcpConnection),
}

#[derive(thiserror::Error, Debug, From)]
pub(crate) enum ServerError {
    #[error("{0}")]
    TcpConnection(error::TcpConnection),
}

impl Machine<Executing, ClientExecutingState> {
    pub(crate) async fn execute_job(
        mut self,
    ) -> Result<
        super::ClientEitherPrepareBuild<Machine<SendFiles, ClientSendFilesState>>,
        (Self, ClientError),
    > {
        // TODO: this broadcast can be made a oneshot
        let (tx_cancel, mut rx_cancel) = broadcast::channel(1);
        let (tx_result, rx_result) = oneshot::channel();

        let working_dir = self.state.working_dir.clone();
        let mut folder_state = self.state.folder_state;
        let job = self.state.job.clone();

        tokio::spawn(async move {
            let msg = match job {
                transport::JobOpt::Python(python_job) => {
                    let run_result =
                        client::run_python_job(python_job, &working_dir, &mut rx_cancel).await;
                    ClientMsg::from_run_result(run_result)
                }
                transport::JobOpt::Apptainer(apptainer_job) => {
                    let run_result = client::run_apptainer_job(
                        apptainer_job,
                        &working_dir,
                        &mut rx_cancel,
                        &mut folder_state,
                    )
                    .await;
                    ClientMsg::from_run_result(run_result)
                }
            };

            tx_result.send((folder_state, msg)).ok().unwrap();
        });

        // TODO: handle cancellations as well here

        // TODO: this gets more complex if the job is cancelled since we dont get
        // our folder state back for free - BUT: i think this might get automatically
        // handled from the job execution perspective
        let (folder_state, msg_result) = rx_result.await.unwrap();
        self.state.folder_state = folder_state;

        let tmp = self.state.conn.transport_data(&msg_result).await;
        throw_error_with_self!(tmp, self);

        match msg_result {
            ClientMsg::CancelledJob => {
                // go to Machine<PrepareBuild, _>
                let prepare_build = self.into_prepare_build_state().await;
                let machine = Machine::from_state(prepare_build);
                let either = Either::Left(machine);
                Ok(either)
            }
            ClientMsg::SuccessfulJob | ClientMsg::FailedJob => {
                // go to Machine<SendFiles, _>
                let send_files = self.into_send_files_state().await;
                let machine = Machine::from_state(send_files);
                let either = Either::Right(machine);
                Ok(either)
            }
        }
    }

    pub(crate) fn to_uninit(self) -> super::UninitClient {
        let ClientExecutingState {
            conn, working_dir, ..
        } = self.state;
        let conn = conn.update_state();
        let state = super::uninit::ClientUninitState { conn, working_dir };
        debug!("moving client executing -> uninit");
        Machine::from_state(state)
    }

    async fn into_send_files_state(self) -> super::send_files::ClientSendFilesState {
        debug!("moving client executing -> send files");
        let ClientExecutingState {
            conn,
            working_dir,
            job,
            folder_state,
        } = self.state;

        #[allow(unused_mut)]
        let mut conn = conn.update_state();

        #[cfg(test)]
        assert!(conn.bytes_left().await == 0);

        let job_name = job.name().to_string();
        super::send_files::ClientSendFilesState {
            conn,
            working_dir,
            job_name,
            folder_state,
        }
    }

    async fn into_prepare_build_state(self) -> super::prepare_build::ClientPrepareBuildState {
        let ClientExecutingState {
            conn, working_dir, ..
        } = self.state;
        debug!("moving client executing -> prepare build");

        #[allow(unused_mut)]
        let mut conn = conn.update_state();

        #[cfg(test)]
        assert!(conn.bytes_left().await == 0);
        super::prepare_build::ClientPrepareBuildState { conn, working_dir }
    }
}

impl Machine<Executing, ServerExecutingState> {
    pub(crate) async fn wait_job_execution(
        mut self,
    ) -> Result<
        super::ServerEitherPrepareBuild<Machine<SendFiles, ServerSendFilesState>>,
        (Self, ServerError),
    > {
        let msg = self.state.conn.receive_data().await;
        let msg = throw_error_with_self!(msg, self);

        match msg {
            ClientMsg::CancelledJob => {
                // go to Machine<PrepareBuild, _>
                let prepare_build = self.into_prepare_build_state().await;
                let machine = Machine::from_state(prepare_build);
                let either = Either::Left(machine);
                Ok(either)
            }
            ClientMsg::SuccessfulJob | ClientMsg::FailedJob => {
                // go to Machine<SendFiles, _>
                let send_files = self.into_send_files_state().await;
                let machine = Machine::from_state(send_files);
                let either = Either::Right(machine);
                Ok(either)
            }
        }
    }

    pub(crate) fn to_uninit(self) -> super::UninitServer {
        let ServerExecutingState { conn, common, .. } = self.state;
        let conn = conn.update_state();
        let state = super::uninit::ServerUninitState { conn, common };
        debug!("moving client executing -> uninit");
        Machine::from_state(state)
    }

    async fn into_send_files_state(self) -> super::send_files::ServerSendFilesState {
        debug!(
            "moving {} server executing -> send files",
            self.state.common.node_name
        );
        let ServerExecutingState {
            conn,
            common,
            namespace,
            batch_name,
            job_identifier,
            job_name,
            save_location,
        } = self.state;

        #[allow(unused_mut)]
        let mut conn = conn.update_state();

        #[cfg(test)]
        assert!(conn.bytes_left().await == 0);

        super::send_files::ServerSendFilesState {
            conn,
            common,
            namespace,
            batch_name,
            job_identifier,
            job_name,
            save_location,
        }
    }

    async fn into_prepare_build_state(self) -> super::prepare_build::ServerPrepareBuildState {
        debug!(
            "moving {} server executing -> prepare build",
            self.state.common.node_name
        );
        let ServerExecutingState { conn, common, .. } = self.state;

        #[allow(unused_mut)]
        let mut conn = conn.update_state();

        #[cfg(test)]
        assert!(conn.bytes_left().await == 0);

        super::prepare_build::ServerPrepareBuildState { conn, common }
    }
}

#[derive(Serialize, Deserialize, Unwrap)]
pub(crate) enum ServerMsg {}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub(crate) enum ClientMsg {
    CancelledJob,
    SuccessfulJob,
    FailedJob,
}

impl ClientMsg {
    fn from_run_result(execution_output: Result<Option<()>, crate::Error>) -> Self {
        match execution_output {
            Ok(None) => Self::CancelledJob,
            Ok(Some(_)) => Self::SuccessfulJob,
            Err(_e) => Self::FailedJob,
        }
    }
}

impl transport::AssociatedMessage for ServerMsg {
    type Receive = ClientMsg;
}

impl transport::AssociatedMessage for ClientMsg {
    type Receive = ServerMsg;
}
