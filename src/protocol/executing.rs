use super::Machine;
use crate::prelude::*;
use super::Either;

use super::send_files::{SendFiles, ServerSendFilesState, ClientSendFilesState};

pub(crate) struct Executing;
pub(crate) struct ClientExecutingState {
    conn: transport::FollowerConnection<ClientMsg>,
    working_dir: PathBuf,
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
    #[error("Node failed the keepalive check")]
    MissedKeepalive,
}

impl Machine<Executing, ClientExecutingState> {
    pub(crate) async fn execute_job(mut self) -> Result<super::ServerEitherPrepareBuild<Machine<SendFiles,ServerSendFilesState>>, Error> {
        todo!()
    }
}

impl Machine<Executing, ServerExecutingState> {
    pub(crate) async fn wait_job_execution(mut self) -> Result<super::ServerEitherPrepareBuild<Machine<SendFiles,ServerSendFilesState>>, Error> {
        todo!()
    }
}

#[derive(Serialize, Deserialize, Unwrap)]
pub(crate) enum ServerMsg { }

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub(crate) enum ClientMsg {}

impl transport::AssociatedMessage for ServerMsg {
    type Receive = ClientMsg;
}

impl transport::AssociatedMessage for ClientMsg {
    type Receive = ServerMsg;
}
