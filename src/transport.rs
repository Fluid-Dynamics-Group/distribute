use derive_more::{Constructor, Display};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::error;
use crate::config;
use crate::error::Error;
use crate::server;
use bincode::config::Options;

#[derive(
    Deserialize, Serialize, Debug, Clone, PartialEq, derive_more::From, derive_more::Unwrap,
)]
pub enum RequestFromServer {
    StatusCheck,
    InitPythonJob(PythonJobInit),
    RunPythonJob(PythonJob),
    InitSingularityJob(SingularityJobInit),
    RunSingularityJob(SingularityJob),
    FileReceived,
    KillJob,
}

impl From<crate::server::JobOpt> for RequestFromServer {
    fn from(x:crate::server::JobOpt) -> Self {
        match x {
            crate::server::JobOpt::Python(p) => Self::RunPythonJob(p),
            crate::server::JobOpt::Singularity(s) => Self::RunSingularityJob(s),
        }
    }
}

impl From<config::BuildOpts> for RequestFromServer {
    fn from(x: config::BuildOpts) -> Self {
        match x {
            config::BuildOpts::Python(p) => Self::InitPythonJob(p),
            config::BuildOpts::Singularity(s) => Self::InitSingularityJob(s),
        }
    }
}

#[derive(Deserialize, Serialize, Debug, Clone, derive_more::From, derive_more::Unwrap)]
pub(crate) enum UserMessageToServer {
    AddJobSet(server::OwnedJobSet),
    QueryCapabilities,
    QueryJobNames,
    KillJob(String),
}

#[derive(Deserialize, Serialize, Debug, Clone, derive_more::From, derive_more::Unwrap, Display)]
pub(crate) enum ServerResponseToUser {
    #[display(fmt = "job set added")]
    JobSetAdded,
    #[display(fmt = "job set failed to add")]
    JobSetAddedFailed,
    #[display(fmt = "capabilities")]
    Capabilities(Vec<server::Requirements<server::NodeProvidedCaps>>),
    #[display(fmt = "job names")]
    JobNames(Vec<server::RemainingJobs>),
    #[display(fmt = "job names failed to query")]
    JobNamesFailed,
}

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
pub struct PythonJobInit {
    pub batch_name: String,
    pub python_setup_file: Vec<u8>,
    pub additional_build_files: Vec<File>,
}

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
pub struct File {
    pub file_name: String,
    pub file_bytes: Vec<u8>,
}

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
pub struct PythonJob {
    pub python_file: Vec<u8>,
    pub job_name: String,
    pub job_files: Vec<File>,
}

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
pub struct SingularityJobInit {
    pub batch_name: String,
    pub sif_bytes: Vec<u8>,
    pub build_files: Vec<File>,
}

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
pub struct SingularityJob {
    pub job_name: String,
    pub job_files: Vec<File>,
}

#[derive(Deserialize, Serialize, Display, Debug)]
pub enum ClientResponse {
    #[display(fmt = "status check: _0.display()")]
    StatusCheck(StatusResponse),
    #[display(fmt = "send file: _0.display()")]
    SendFile(SendFile),
    #[display(fmt = "request new job: _0.display()")]
    RequestNewJob(NewJobRequest),
    #[display(fmt = "client error: _0.display()")]
    Error(ClientError),
}

#[derive(Deserialize, Serialize, Display, Constructor, Debug)]
#[display(fmt = "version: {} ready: {}", version, ready)]
pub struct StatusResponse {
    pub version: Version,
    pub ready: bool,
}

impl PartialEq<Version> for StatusResponse {
    fn eq(&self, other: &Version) -> bool {
        self.version == *other
    }
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Display, Debug)]
#[display(fmt = "{}.{}.{}", major, minor, patch)]
pub struct Version {
    major: u16,
    minor: u16,
    patch: u16,
}

impl Version {
    pub fn current_version() -> Self {
        // TODO: pull this from cargo.toml
        Self {
            major: 0,
            minor: 2,
            patch: 0,
        }
    }
}

#[derive(Deserialize, Serialize, Debug)]
pub struct FinishedJob;

#[derive(Deserialize, Serialize, Debug)]
pub struct SendFile {
    pub file_path: PathBuf,
    pub is_file: bool,
    pub bytes: Vec<u8>,
}

#[derive(Deserialize, Serialize, Debug, Constructor, Clone, PartialEq)]
pub struct PauseExecution {
    pub duration: std::time::Duration,
}

#[derive(Deserialize, Serialize, Debug, Constructor, Clone, PartialEq)]
pub struct ResumeExecution;

#[derive(Deserialize, Serialize, Debug)]
pub struct NewJobRequest;

#[derive(Deserialize, Serialize, Debug)]
pub enum ClientError {
    NotReady,
}

fn serialization_options() -> bincode::config::DefaultOptions {
    bincode::config::DefaultOptions::new()
}

#[derive(derive_more::From)]
pub struct ServerConnection {
    conn: TcpStream,
    pub addr: std::net::SocketAddr,
}

impl ServerConnection {
    pub(crate) async fn new(addr: SocketAddr) -> Result<Self, error::TcpConnection> {
        let conn = TcpStream::connect(addr)
            .await
            .map_err(error::TcpConnection::from)?;

        Ok(Self { conn, addr })
    }

    pub(crate) async fn transport_data(
        &mut self,
        request: &RequestFromServer,
    ) -> Result<(), Error> {
        transport(&mut self.conn, request).await
    }

    pub(crate) async fn receive_data(&mut self) -> Result<ClientResponse, Error> {
        receive(&mut self.conn).await
    }

    pub(crate) async fn reconnect(&mut self) -> Result<(), Error> {
        let conn = TcpStream::connect(self.addr)
            .await
            .map_err(error::TcpConnection::from)?;
        self.conn = conn;

        Ok(())
    }
}

pub struct UserConnectionToServer {
    conn: TcpStream,
    pub addr: std::net::SocketAddr,
}

impl UserConnectionToServer {
    pub(crate) async fn new(addr: SocketAddr) -> Result<Self, error::TcpConnection> {
        let conn = TcpStream::connect(addr)
            .await
            .map_err(error::TcpConnection::from)?;

        Ok(Self { conn, addr })
    }

    pub(crate) async fn transport_data(
        &mut self,
        request: &UserMessageToServer,
    ) -> Result<(), Error> {
        transport(&mut self.conn, request).await
    }

    pub(crate) async fn receive_data(&mut self) -> Result<ServerResponseToUser, Error> {
        receive(&mut self.conn).await
    }
}

#[derive(derive_more::From, derive_more::Constructor)]
pub(crate) struct ServerConnectionToUser {
    conn: TcpStream,
}

impl ServerConnectionToUser {
    pub(crate) async fn transport_data(
        &mut self,
        request: &ServerResponseToUser,
    ) -> Result<(), Error> {
        transport(&mut self.conn, request).await
    }

    pub(crate) async fn receive_data(&mut self) -> Result<UserMessageToServer, Error> {
        receive(&mut self.conn).await
    }
}

#[derive(derive_more::Constructor)]
pub struct ClientConnection {
    conn: TcpStream,
}

impl ClientConnection {
    pub(crate) async fn transport_data(&mut self, response: &ClientResponse) -> Result<(), Error> {
        transport(&mut self.conn, response).await
    }

    pub(crate) async fn receive_data(&mut self) -> Result<RequestFromServer, Error> {
        receive(&mut self.conn).await
    }
}

async fn transport<T: Serialize>(tcp_connection: &mut TcpStream, data: &T) -> Result<(), Error> {
    let serializer = serialization_options();
    let bytes = serializer
        .serialize(&data)
        .map_err(error::Serialization::from)?;

    debug!("sending buffer of length {}", bytes.len());

    let bytes_len: u64 = bytes.len() as u64;

    // write the length of the data that we are first sending
    tcp_connection
        .write_all(&bytes_len.to_le_bytes())
        .await
        .map_err(error::TcpConnection::from)?;

    // write the contents of the actual data now that the length of the data is
    // actually known
    tcp_connection
        .write_all(&bytes)
        .await
        .map_err(error::TcpConnection::from)?;

    Ok(())
}

async fn receive<T: DeserializeOwned>(tcp_connection: &mut TcpStream) -> Result<T, Error> {
    let deserializer = serialization_options();

    let mut buf: [u8; 8] = [0; 8];

    read_buffer_bytes(&mut buf, tcp_connection).await?;

    let content_length = u64::from_le_bytes(buf);

    debug!("receiving buffer with length {}", content_length);

    let mut content_buffer = vec![0; content_length as usize];

    read_buffer_bytes(&mut content_buffer, tcp_connection).await?;

    let output = deserializer
        .deserialize(&content_buffer)
        .map_err(error::Deserialization::from)?;

    Ok(output)
}

async fn read_buffer_bytes(buffer: &mut [u8], conn: &mut TcpStream) -> Result<(), Error> {
    let mut starting_idx = 0;

    loop {
        debug!(
            "reading buffer bytes w/ index {} and buffer len {}",
            starting_idx,
            buffer.len()
        );

        let bytes_read = conn
            .read(&mut buffer[starting_idx..])
            .await
            .map_err(error::TcpConnection::from)?;

        // this should mean that the other party has closed the connection
        if bytes_read == 0 {
            return Err(Error::TcpConnection(error::TcpConnection::ConnectionClosed));
        }

        starting_idx += bytes_read;

        if starting_idx == buffer.len() {
            break;
        }
    }

    debug!("finished reading bytes from buffer");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::convert::TryInto;
    use std::net::SocketAddr;
    use std::time::{Duration, Instant};
    use tokio::net::TcpListener;

    fn add_port(port: u16) -> SocketAddr {
        SocketAddr::from(([0, 0, 0, 0], port))
    }

    /// make sure that if we are just waiting on more bytes it doesnt
    /// mean that we have hit an EOF (that the other connection
    #[tokio::test]
    async fn ensure_not_eof() {
        let client_listener = TcpListener::bind(add_port(9994)).await.unwrap();
        let mut raw_server_connection = TcpStream::connect(add_port(9994)).await.unwrap();
        let mut client_connection =
            ClientConnection::new(client_listener.accept().await.unwrap().0);

        let file_bytes = (0..255).into_iter().collect::<Vec<u8>>();
        let server_req = RequestFromServer::AssignJob(Job {
            python_file: file_bytes,
            job_name: "ensure_not_eof".into(),
        });

        // serialize the data manually
        let serializer = serialization_options();
        let server_req_bytes: Vec<u8> = serializer.serialize(&server_req).unwrap();

        let bytes_len = server_req_bytes.len();
        let first_section = server_req_bytes[0..bytes_len / 2].to_vec();
        let second_section = server_req_bytes[bytes_len / 2..].to_vec();
        assert_eq!(first_section.len() + second_section.len(), bytes_len);

        // start counting the number of seconds that have passed
        let start = Instant::now();

        // on another thread, send the whole contents of the data over 2
        // seconds to see if the main thread will panic from missing info
        tokio::task::spawn(async move {
            let content_length_bytes = u64::to_le_bytes(bytes_len.try_into().unwrap());
            raw_server_connection
                .write_all(&content_length_bytes)
                .await
                .unwrap();
            raw_server_connection
                .write_all(&first_section)
                .await
                .unwrap();

            std::thread::sleep(Duration::from_secs(2));

            raw_server_connection
                .write_all(&second_section)
                .await
                .unwrap();
        });

        let client_version_of_request = client_connection.receive_data().await;
        let elapsed = start.elapsed();

        dbg!(elapsed.as_secs_f64());

        assert!(elapsed.as_secs_f64() > 2.);
        assert_eq!(client_version_of_request.unwrap(), server_req);
    }

    /// make sure that if we close the connection an EOF is reached
    #[tokio::test]
    async fn ensure_eof_on_close() {
        let client_listener = TcpListener::bind(add_port(9995)).await.unwrap();
        let addr = add_port(9995);
        let server_connection =
            ServerConnection::from((TcpStream::connect(addr).await.unwrap(), addr));

        let mut client_connection =
            ClientConnection::new(client_listener.accept().await.unwrap().0);

        std::mem::drop(server_connection);

        let rx = client_connection.receive_data().await;

        // ensure its `error::TcpConnection::ConnectionClosed`
        match rx.unwrap_err() {
            error::Error::TcpConnection(tcp) => {
                // make sure its a connection closed error
                tcp.unwrap_connection_closed();
            }
            x => {
                panic!("Error {:?} was not TcpConnection::ConnectionClosed", x);
            }
        };
    }

    #[tokio::test]
    async fn no_send_result() {
        let client_listener = TcpListener::bind(add_port(9996)).await.unwrap();
        let addr = add_port(9996);
        let mut server_connection =
            ServerConnection::from((TcpStream::connect(addr).await.unwrap(), addr));

        let _client_connection = ClientConnection::new(client_listener.accept().await.unwrap().0);

        let timeout = Duration::from_secs(3);
        let fut = server_connection.receive_data();
        let output = tokio::time::timeout(timeout, fut).await;

        assert_eq!(output.is_err(), true);
    }

    #[test]
    fn build_file_truncate() {
        let build_file = std::iter::repeat(0).take(20714).collect::<Vec<u8>>();
        let serializer = serialization_options();
        let x = serializer.serialize(&build_file).unwrap();
        let y: Vec<u8> = serializer.deserialize(&x).unwrap();
        assert_eq!(build_file.len(), y.len());
    }
}
