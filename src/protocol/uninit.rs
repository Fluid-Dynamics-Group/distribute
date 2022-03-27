use super::prepare_build;
use super::Machine;
use crate::prelude::*;

#[derive(thiserror::Error, Debug, From)]
pub(crate) enum ServerError {
    #[error("node version did not match server version: `{0}`")]
    VersionMismatch(transport::Version),
    #[error("{0}")]
    TcpConnection(error::TcpConnection),
    #[error("Unexpected Response: {0}")]
    Response(ClientMsg),
}

#[derive(thiserror::Error, Debug, From)]
pub(crate) enum ClientError {
    #[error("node version did not match server version. Server has discontinued the connection")]
    VersionMismatch,
    #[error("{0}")]
    TcpConnection(error::TcpConnection),
}

impl Machine<Uninit, ClientUninitState> {
    /// on the compute node, wait for a connection from the host and load the connected state
    /// once it has been made
    pub(crate) async fn connect_to_host(
        mut self,
    ) -> Result<
        Machine<prepare_build::PrepareBuild, prepare_build::ClientPrepareBuildState>,
        (Self, ClientError),
    > {
        // first message should be querying what our version is
        let msg = self.state.conn.receive_data().await;

        let msg: ServerMsg = throw_error_with_self!(msg, self);

        debug!(
            "expecting version message, got: {:?} - sending version response",
            msg
        );
        let response = ClientMsg::ResponseVersion(transport::Version::current_version());

        throw_error_with_self!(self.state.conn.transport_data(&response).await, self);

        // now the server will either tell us the versions are the same, or
        // that they dont match and we cannot talk
        let msg_result = self.state.conn.receive_data().await;

        let msg = throw_error_with_self!(msg_result, self);

        match msg {
            ServerMsg::RequestVersion => {
                error!("client asked for version information after we sent it. This is an unreachable state");
                unreachable!()
            }
            ServerMsg::VersionMismatch => return Err((self, ClientError::VersionMismatch)),
            ServerMsg::VersionsMatched => {
                debug!("versions matched - continuing to next step");
                // TODO: enter next state machine
                todo!()
            }
        }
    }

    pub(crate) fn new(conn: tokio::net::TcpStream) -> Self {
        let conn = transport::Connection::from_connection(conn);
        let state = ClientUninitState { conn };

        Self {
            state,
            _marker: Uninit,
        }
    }
}

impl Machine<Uninit, ServerUninitState> {
    /// on the master node, try to connect to the compute node
    pub(crate) async fn connect_to_node(
        mut self,
    ) -> Result<
        Machine<prepare_build::PrepareBuild, prepare_build::ServerPrepareBuildState>,
        (Self, ServerError),
    > {
        let our_version = transport::Version::current_version();

        let version_request = ServerMsg::RequestVersion;
        throw_error_with_self!(self.state.conn.transport_data(&version_request).await, self);

        // grab the version information
        let msg = throw_error_with_self!(self.state.conn.receive_data().await, self);
        let ClientMsg::ResponseVersion(client_version) = msg;

        // check that the client is running the same verison of the program as us
        if our_version != client_version {
            throw_error_with_self!(
                self.state
                    .conn
                    .transport_data(&ServerMsg::VersionMismatch)
                    .await,
                self
            );
            return Err((self, ServerError::VersionMismatch(client_version)));
        }

        // tell the client that we are moving forward with the connection
        throw_error_with_self!(
            self.state
                .conn
                .transport_data(&ServerMsg::VersionsMatched)
                .await,
            self
        );

        // set to the new state
        todo!()
    }
}

pub(crate) struct Uninit;

pub(crate) struct ClientUninitState {
    conn: transport::Connection<ClientMsg>,
}

pub(crate) struct ServerUninitState {
    client_ip: SocketAddr,
    client_name: String,
    conn: transport::Connection<ServerMsg>,
    common: super::Common,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub(crate) enum ServerMsg {
    RequestVersion,
    VersionsMatched,
    VersionMismatch,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Display)]
pub(crate) enum ClientMsg {
    #[display(fmt = "version message: {}", _0)]
    ResponseVersion(transport::Version),
}

impl transport::AssociatedMessage for ServerMsg {
    type Receive = ClientMsg;
}

impl transport::AssociatedMessage for ClientMsg {
    type Receive = ServerMsg;
}
