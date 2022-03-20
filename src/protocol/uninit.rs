use super::prepare_build;
use super::Machine;
use crate::prelude::*;

#[derive(thiserror::Error, Debug, From)]
enum Error {
    #[error("node version did not match server version: `{0}`")]
    VersionMismatch(transport::Version),
    #[error("{0}")]
    TcpConnection(error::TcpConnection),
    #[error("unexpected request: {actual:?}, expected {expected:?}")]
    ClientUnexpected {
        actual: ServerMsg,
        expected: FlatServerMsg,
    },
    #[error("unexpected request: {actual:?}, expected {expected:?}")]
    ServerUnexpected {
        actual: ClientMsg,
        expected: FlatClientMsg,
    },
}

impl Machine<Uninit, ClientUninitState> {
    /// on the compute node, wait for a connection from the host and load the connected state
    /// once it has been made
    async fn connect_to_host(
        mut self,
    ) -> Result<Machine<prepare_build::PrepareBuild, prepare_build::ClientPrepareBuildState>, Error>
    {
        // first message should be querying what our version is
        let msg = self.state.conn.receive_data().await?;
        if ServerMsg::RequestVersion != msg {
            // we did not get the message we expected
            return Err(Error::ClientUnexpected {
                actual: msg,
                expected: FlatServerMsg::RequestVersion,
            });
        }

        // second message _should_ be that we are moving forward.
        // if the server cancelled the connection then this should result in a tcp connection
        // error
        let msg = self.state.conn.receive_data().await?;
        if ServerMsg::RequestConfirm != msg {
            // we did not get the message we expected
            return Err(Error::ClientUnexpected {
                actual: msg,
                expected: FlatServerMsg::RequestConfirm,
            });
        }

        // TODO: set up the next state
        todo!()
    }

    pub(crate) fn new(conn: tokio::net::TcpStream) -> Self {
        let conn = transport::Connection::from_connection(conn);
        let state = ClientUninitState {
            conn
        };

        Self {
            state,
            _marker: Uninit,
        }
    }
}

impl Machine<Uninit, ServerUninitState> {
    /// on the master node, try to connect to the compute node
    async fn connect_to_node(
        mut self,
    ) -> Result<Machine<prepare_build::PrepareBuild, prepare_build::ServerPrepareBuildState>, Error>
    {
        let our_version = transport::Version::current_version();

        let version_request = ServerMsg::RequestVersion;
        self.state.conn.transport_data(&version_request).await?;

        let msg = self.state.conn.receive_data().await?;
        let client_version = if let ClientMsg::ResponseVersion(version) = msg {
            version
        } else {
            return Err(Error::ServerUnexpected {
                actual: msg,
                expected: FlatClientMsg::ResponseVersion,
            });
        };

        // check that the client is running the same verison of the program as us
        if our_version != client_version {
            self.state
                .conn
                .transport_data(&ServerMsg::VersionMismatch)
                .await?;
            return Err(Error::VersionMismatch(client_version));
        }

        // tell the client that we are moving forward with the connection
        self.state
            .conn
            .transport_data(&ServerMsg::RequestConfirm)
            .await?;
        // recieve the client telling us that we are ready to move forward
        let msg = self.state.conn.receive_data().await?;

        if let ClientMsg::RespondConfirm = msg {
            //
        } else {
            return Err(Error::ServerUnexpected {
                actual: msg,
                expected: FlatClientMsg::RespondConfirm,
            });
        }

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
    RequestConfirm,
    VersionMismatch,
}

#[derive(Debug)]
pub(crate) enum FlatServerMsg {
    RequestVersion,
    RequestConfirm,
    VersionMismatch,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub(crate) enum ClientMsg {
    ResponseVersion(transport::Version),
    RespondConfirm,
}

#[derive(Debug)]
pub(crate) enum FlatClientMsg {
    ResponseVersion,
    RespondConfirm,
}

impl transport::AssociatedMessage for ServerMsg {
    type Receive = ClientMsg;
}

impl transport::AssociatedMessage for ClientMsg {
    type Receive = ServerMsg;
}
