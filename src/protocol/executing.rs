use super::Machine;
use crate::prelude::*;
use super::Either;

use super::send_files::{SendFiles, ServerSendFilesState, ClientSendFilesState};

pub(crate) struct Executing;
pub(crate) struct ClientExecutingState {
    conn: transport::FollowerConnection<ClientMsg>,
    working_dir: PathBuf,
    job: server::JobOpt,
    folder_state: client::BindingFolderState,
}

pub(crate) struct ServerExecutingState  {
    conn: transport::ServerConnection<ServerMsg>,
    common: super::Common,
    namespace: String,
    batch_name: String,
    // the job identifier we have scheduled to run
    job_identifier: server::JobIdentifier
}

#[derive(thiserror::Error, Debug, From)]
pub(crate) enum Error {
    #[error("{0}")]
    TcpConnection(error::TcpConnection),
}

impl Machine<Executing, ClientExecutingState> {
    pub(crate) async fn execute_job(mut self) -> Result<super::ServerEitherPrepareBuild<Machine<SendFiles, ClientSendFilesState>>, Error> {
        // TODO: this broadcast can be made a oneshot
        let (tx_cancel, mut rx_cancel) = broadcast::channel(1);
        let (tx_result, rx_result) = oneshot::channel();

        let working_dir = self.state.working_dir.clone();
        let mut folder_state = self.state.folder_state;

        tokio::spawn(async move {
            let msg = match self.state.job {
                server::JobOpt::Python(python_job) => {
                    let run_result = client::run_python_job(python_job, &working_dir, &mut rx_cancel).await;
                    ClientMsg::from_run_result(run_result)
                }
                server::JobOpt::Singularity(singularity_job) => {
                    let run_result = client::run_singularity_job(singularity_job, &working_dir, &mut rx_cancel, &mut folder_state).await;
                    ClientMsg::from_run_result(run_result)
                }
            };

            tx_result.send((folder_state, msg)).ok().unwrap();
            
        });

        // TODO: handle cancellations as well here
        let (folder_state, msg_result) = rx_result.await.unwrap();

        self.state.conn.transport_data(&msg_result).await?;

        match msg_result {
            ClientMsg::CancelledJob => {
                // go to Machine<PrepareBuild, _>
            }
            ClientMsg::SuccessfulJob | ClientMsg::FailedJob => {
                // go to Machine<SendFiles, _>
            }
        }


        todo!()
    }
}

impl Machine<Executing, ServerExecutingState> {
    pub(crate) async fn wait_job_execution(mut self) -> Result<super::ServerEitherPrepareBuild<Machine<SendFiles,ServerSendFilesState>>, Error> {
        let msg = self.state.conn.receive_data().await?;

        match msg {
            ClientMsg::CancelledJob => {
                // go to Machine<PrepareBuild, _>
            }
            ClientMsg::SuccessfulJob | ClientMsg::FailedJob => {

            }
        }

        todo!()
    }
}

#[derive(Serialize, Deserialize, Unwrap)]
pub(crate) enum ServerMsg { }

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
            Err(_e) => {
                Self::FailedJob
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
