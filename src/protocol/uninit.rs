use super::prepare_build;
use super::Machine;
use crate::prelude::*;

#[derive(Default, Debug)]
pub(crate) struct Uninit;

#[derive(Debug)]
pub(crate) struct ClientUninitState {
    pub(super) conn: transport::Connection<ClientMsg>,
    pub(super) working_dir: WorkingDir,
    pub(super) cancel_addr: SocketAddr,
}

#[derive(Debug)]
pub(crate) struct ServerUninitState {
    pub(super) conn: transport::Connection<ServerMsg>,
    pub(super) common: super::Common,
}

#[derive(thiserror::Error, Debug, From)]
pub(crate) enum ServerError {
    #[error("node version did not match server version: `{0}`")]
    VersionMismatch(VersionMismatch),
    #[error("{0}")]
    TcpConnection(error::TcpConnection),
    #[error("Unexpected Response: {0}")]
    Response(ClientMsg),
}

#[derive(thiserror::Error, Debug, From, Display)]
#[display(
    fmt = "server version {} client version {}",
    server_version,
    client_version
)]
pub(crate) struct VersionMismatch {
    server_version: transport::Version,
    client_version: transport::Version,
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
    #[instrument(skip(self), fields(working_dir = %self.state.working_dir))]
    pub(crate) async fn connect_to_host(
        mut self,
    ) -> Result<
        Machine<prepare_build::PrepareBuild, prepare_build::ClientPrepareBuildState>,
        (Self, ClientError),
    > {
        trace!("created uninit client node");

        // first message should be querying what our version is
        let msg = self.state.conn.receive_data().await;

        trace!("received version query on the client");

        let msg: ServerMsg = throw_error_with_self!(msg, self);

        debug!(
            "expecting version message, got: {:?} - sending version response",
            msg
        );
        let response = ClientMsg::ResponseVersion(transport::Version::current_version());

        throw_error_with_self!(self.state.conn.transport_data(&response).await, self);

        trace!("send version query response on the client");

        // now the server will either tell us the versions are the same, or
        // that they dont match and we cannot talk
        let msg_result = self.state.conn.receive_data().await;

        trace!("received veresion query update (either pass / fail the connection) on the client");

        let msg = throw_error_with_self!(msg_result, self);

        match msg {
            ServerMsg::RequestVersion => {
                error!("client asked for version information after we sent it. This is an unreachable state");
                unreachable!()
            }
            ServerMsg::VersionMismatch => Err((self, ClientError::VersionMismatch)),
            ServerMsg::VersionsMatched => {
                debug!("versions matched - continuing to next step");

                // transition to Machine<PrepareBuild, _>
                let state = self.into_prepare_build_state().await;
                let machine = Machine::from_state(state);
                Ok(machine)
            }
        }
    }

    #[instrument(skip(conn))]
    pub(crate) fn new(
        conn: tokio::net::TcpStream,
        working_dir: WorkingDir,
        cancel_addr: SocketAddr,
    ) -> Self {
        debug!("constructing uninitialized client");
        let conn = transport::Connection::from_connection(conn);
        let state = ClientUninitState {
            conn,
            working_dir,
            cancel_addr,
        };

        Self {
            state,
            _marker: Uninit,
        }
    }

    async fn into_prepare_build_state(self) -> super::prepare_build::ClientPrepareBuildState {
        let ClientUninitState {
            conn,
            working_dir,
            cancel_addr,
        } = self.state;
        #[allow(unused_mut)]
        let mut conn = conn.update_state();

        #[cfg(test)]
        assert!(conn.bytes_left().await == 0);

        debug!("moving client uninit -> prepare build");
        super::prepare_build::ClientPrepareBuildState {
            conn,
            working_dir,
            cancel_addr,
        }
    }
}

impl Machine<Uninit, ServerUninitState> {
    /// on the master node, try to connect to the compute node
    #[instrument(
        skip(self), 
        fields(
            node_meta = %self.state.common.node_meta,
        )
    )]
    pub(crate) async fn connect_to_node(
        mut self,
    ) -> Result<
        Machine<prepare_build::PrepareBuild, prepare_build::ServerPrepareBuildState>,
        (Self, ServerError),
    > {
        let our_version = transport::Version::current_version();

        trace!("sending version request query from the server");

        let version_request = ServerMsg::RequestVersion;
        throw_error_with_self!(self.state.conn.transport_data(&version_request).await, self);

        // grab the version information
        let msg = throw_error_with_self!(self.state.conn.receive_data().await, self);
        let ClientMsg::ResponseVersion(client_version) = msg;

        trace!("received version number from the client");

        // check that the client is running the same verison of the program as us
        if our_version != client_version {
            throw_error_with_self!(
                self.state
                    .conn
                    .transport_data(&ServerMsg::VersionMismatch)
                    .await,
                self
            );

            let mismatch = VersionMismatch {
                server_version: our_version,
                client_version,
            };

            return Err((self, ServerError::VersionMismatch(mismatch)));
        }

        // tell the client that we are moving forward with the connection
        throw_error_with_self!(
            self.state
                .conn
                .transport_data(&ServerMsg::VersionsMatched)
                .await,
            self
        );

        // transition to Machine<PrepareBuild, _>
        let prepare_build_state = self.into_prepare_build_state().await;
        let machine = Machine::from_state(prepare_build_state);

        Ok(machine)
    }

    pub(crate) fn new(conn: tokio::net::TcpStream, common: super::Common) -> Self {
        debug!("constructing uninitialized server");
        let conn = transport::Connection::from_connection(conn);

        let state = ServerUninitState { common, conn };

        Self {
            state,
            _marker: Uninit,
        }
    }

    async fn into_prepare_build_state(self) -> super::prepare_build::ServerPrepareBuildState {
        debug!(
            "moving {} server uninit -> prepare build",
            self.state.common.node_meta
        );
        let ServerUninitState { conn, common } = self.state;
        #[allow(unused_mut)]
        let mut conn = conn.update_state();

        #[cfg(test)]
        assert!(conn.bytes_left().await == 0);

        super::prepare_build::ServerPrepareBuildState { conn, common }
    }

    /// update the underlying TCP connection that the state machine will use
    ///
    /// This method *must* be called if there was an error with the previous TCP
    /// connection, since the previous TCP connection will continue to indefinitely
    /// error if it is not set to a new value (it will be closed).
    pub(crate) fn update_tcp_connection(&mut self, conn: tokio::net::TcpStream) {
        info!("Setting up new TCP connection to compute node on server");
        let conn = transport::Connection::from_connection(conn);

        self.state.conn = conn;
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::Common;
    use tokio::net::{TcpListener, TcpStream};
    use transport::Connection;

    fn add_port(port: u16) -> SocketAddr {
        SocketAddr::from(([0, 0, 0, 0], port))
    }

    #[tokio::test]
    async fn uninit() {
        let transport_port = 9997;
        let keepalive_port = 9998;
        let client_transport_addr = add_port(transport_port);
        let client_keepalive_addr = add_port(keepalive_port);
        // other addrs taken elsewhere
        let cancel_addr = add_port(10_003);

        let working_dir = WorkingDir::from(PathBuf::from("./tests/unittests"));
        let client_listener = TcpListener::bind(client_transport_addr).await.unwrap();
        let raw_server_connection = TcpStream::connect(client_transport_addr).await.unwrap();

        let client_connection =
            Connection::from_connection(client_listener.accept().await.unwrap().0);

        let server_connection = Connection::from_connection(raw_server_connection);

        let (_tx_cancel, common) =
            Common::test_configuration(client_transport_addr, client_keepalive_addr, cancel_addr);

        let client_state = ClientUninitState {
            conn: client_connection,
            working_dir,
            cancel_addr,
        };
        let server_state = ServerUninitState {
            conn: server_connection,
            common,
        };

        // create the machines from the states
        let client: Machine<Uninit, _> = Machine::from_state(client_state);
        let server: Machine<Uninit, _> = Machine::from_state(server_state);

        // create some oneshot channels for the spawned tasks
        // to return the information to the main process
        let (tx_client, rx_client) = oneshot::channel::<bool>();
        let (tx_server, rx_server) = oneshot::channel::<bool>();

        // run the server on its own process
        let server = tokio::spawn(async move {
            match server.connect_to_node().await {
                // we arrived at the next state correctly
                Ok(mut next_state) => {
                    eprintln!("server finished successfully");

                    if next_state.state.conn.bytes_left().await > 0 {
                        eprintln!("bytes remain in the server connection - this will cause errors in the next state");

                        tx_server.send(false).unwrap();
                    } else {
                        eprintln!("no bytes are remaining in server connection");
                        tx_server.send(true).unwrap();
                    }
                }
                Err((_self, e)) => {
                    eprintln!("server crashed: {:?}", e);
                    tx_server.send(false).unwrap();
                }
            }
        });

        // run the client on its own process
        let client = tokio::spawn(async move {
            match client.connect_to_host().await {
                // we arrived at the next state correctly
                Ok(mut next_state) => {
                    eprintln!("client finished successfully");

                    if next_state.state.conn.bytes_left().await > 0 {
                        eprintln!("bytes remain in the client connection - this will cause errors in the next state");

                        tx_client.send(false).unwrap();
                    } else {
                        eprintln!("no bytes are remaining in client connection");
                        tx_client.send(true).unwrap();
                    }
                }
                Err((_self, e)) => {
                    eprintln!("client crashed: {:?}", e);
                    tx_client.send(false).unwrap();
                }
            }
        });

        // execute the tasks
        futures::future::join_all([server, client]).await;

        // check that the client returns something
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(5), rx_client)
                .await
                .unwrap()
                .unwrap(),
            true
        );

        // check that the server returns something
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(5), rx_server)
                .await
                .unwrap()
                .unwrap(),
            true
        );
    }
}
