use crate::prelude::*;

use serde::de::DeserializeOwned;

use std::net::SocketAddr;
use std::path::PathBuf;

use {
    bincode::config::Options,
    tokio::io::{AsyncReadExt, AsyncWriteExt},
    tokio::net::TcpStream,
};

use crate::config::requirements;
use crate::error;
use crate::error::Error;
use crate::server;
use tokio::io::{AsyncRead, AsyncWrite};

#[cfg(not(test))]
const BUFFER_LEN: usize = 1_000_000;
#[cfg(test)]
const BUFFER_LEN: usize = 100;

#[derive(Serialize, Deserialize)]
pub(crate) enum ServerQuery {
    KeepaliveCheck,
    VersionCheck,
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) enum ClientQueryAnswer {
    KeepaliveResponse,
    VersionResponse(Version),
}

/// Helper trait to ensure that `Connection` types send and
/// receive the same expected type for each other
pub(crate) trait AssociatedMessage {
    type Receive;
    const IS_KEEPALIVE: bool = false;
}

mod messages {
    use super::*;

    impl AssociatedMessage for UserMessageToServer {
        type Receive = ServerResponseToUser;
    }

    impl AssociatedMessage for ServerResponseToUser {
        type Receive = UserMessageToServer;
    }

    impl AssociatedMessage for ServerQuery {
        type Receive = ClientQueryAnswer;
        const IS_KEEPALIVE: bool = true;
    }

    impl AssociatedMessage for ClientQueryAnswer {
        type Receive = ServerQuery;
        const IS_KEEPALIVE: bool = true;
    }
}

#[derive(Deserialize, Serialize, Debug, Clone, derive_more::From, derive_more::Unwrap)]
pub enum UserMessageToServer {
    AddJobSet(config::TransportJobs<config::common::HashedFile>),
    QueryCapabilities,
    QueryJobNames,
    KillJob(String),
    PullFilesInitialize(PullFileRequest),
    FileReceived,
}

#[derive(Deserialize, Serialize, Debug, Clone, Constructor)]
pub struct PullFileRequest {
    // regular expressions that are either for matching-for
    // or matching-against the files in the server
    //
    // this behavior is determined by the is_include_filter boolean
    // if is_include_filter is true, then only files matching these filters
    // should be considered
    pub(crate) filters: Vec<String>,
    pub(crate) is_include_filter: bool,
    // the namespace of the job that should be pulled - this is from the yaml job that
    // we parsed
    pub(crate) namespace: String,
    // the batch name from the namespace that we parsed
    pub(crate) batch_name: String,
    // whether or not to only pull matching and non-matching files and skip pulling
    // the actual data
    pub(crate) dry: bool,
    // skip sending all folders since we have built a folder structure previously
    pub(crate) skip_folders: bool,
}

#[derive(Deserialize, Serialize, Debug, derive_more::From, derive_more::Unwrap, Display)]
pub enum ServerResponseToUser {
    #[display(fmt = "job set added")]
    JobSetAdded,
    #[display(fmt = "job set failed to add")]
    JobSetAddedFailed,
    #[display(fmt = "capabilities")]
    Capabilities(Vec<requirements::Requirements<requirements::NodeProvidedCaps>>),
    #[display(fmt = "job names")]
    JobNames(Vec<server::RemainingJobs>),
    #[display(fmt = "job names failed to query")]
    JobNamesFailed,
    #[display(fmt = "Result of removing the job set: {}", "_0")]
    KillJob(crate::server::CancelResult),
    #[display(fmt = "Failed to kill the job set - probably could not communicate to the job pool")]
    KillJobFailed,
    #[display(fmt = "Failed to pull files after error: {}", "_0")]
    PullFilesError(error::PullError),
    #[display(fmt = "Pull dry response {}", "_0")]
    PullFilesDryResponse(PullFilesDryResponse),
    #[display(fmt = "A file was sent at path {}", "_0.file_path.display()")]
    SendFile(SendFile),
    #[display(
        fmt = "A file marker was sent at {} indicating the next data will be raw bytes",
        "_0.file_path.display()"
    )]
    FileMarker(FileMarker),
    #[display(fmt = "Finished sending all files")]
    FinishFiles,
    #[display(fmt = "Continue sending init / job files")]
    Continue,
}

#[derive(Serialize, Debug, Clone, Deserialize, Display, Constructor)]
#[display(
    fmt = "included:\n{:?}\nfiltered:\n{:?}",
    success_files,
    filtered_files
)]
pub struct PullFilesDryResponse {
    pub success_files: Vec<PathBuf>,
    pub filtered_files: Vec<PathBuf>,
}

#[derive(Deserialize, Serialize, Clone, PartialEq, Eq)]
pub struct File {
    pub file_name: String,
    pub file_bytes: Vec<u8>,
}

impl fmt::Debug for File {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("File")
            .field("file_name", &self.file_name)
            .field("file_bytes (length)", &self.file_bytes.len())
            .finish()
    }
}

//#[derive(Deserialize, Serialize, Clone, PartialEq, Eq)]
//pub struct PythonJob {
//    pub python_file: Vec<u8>,
//    pub job_name: String,
//    pub job_files: Vec<File>,
//}
//
//impl fmt::Debug for PythonJob {
//    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
//        fmt.debug_struct("PythonJob")
//            .field("job_name", &self.job_name)
//            .field("job_files", &self.job_files)
//            .field("python_file (length)", &self.python_file.len())
//            .finish()
//    }
//}
//
//#[derive(Deserialize, Serialize, Clone, PartialEq, Eq)]
//pub struct ApptainerJob {
//    pub job_name: String,
//    pub job_files: Vec<File>,
//}
//
//impl fmt::Debug for ApptainerJob {
//    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
//        fmt.debug_struct("ApptainerJob")
//            .field("job_name", &self.job_name)
//            .field("job_files", &self.job_files)
//            .finish()
//    }
//}
//
//#[derive(Clone, PartialEq, From, Debug, Serialize, Deserialize, Eq)]
//pub(crate) enum JobOpt {
//    Apptainer(ApptainerJob),
//    Python(PythonJob),
//}
//
//impl JobOpt {
//    pub(crate) fn name(&self) -> &str {
//        match &self {
//            Self::Apptainer(x) => &x.job_name,
//            Self::Python(x) => &x.job_name,
//        }
//    }
//
//    #[cfg(test)]
//    pub(crate) fn placeholder_test_data() -> Self {
//        transport::JobOpt::Python(transport::PythonJob {
//            python_file: vec![],
//            job_name: "test_job".into(),
//            job_files: vec![],
//        })
//    }
//}

#[derive(Deserialize, Serialize, PartialEq, Eq, Display, Debug)]
#[display(fmt = "{}.{}.{}", major, minor, patch)]
pub struct Version {
    major: u16,
    minor: u16,
    patch: u16,
}

impl Version {
    /// parse the current version of the package from Cargo.toml
    pub fn current_version() -> Self {
        let mut iter = env!("CARGO_PKG_VERSION").split('.');
        let major = iter.next().unwrap().parse().unwrap();
        let minor = iter.next().unwrap().parse().unwrap();
        let patch = iter.next().unwrap().parse().unwrap();

        Self {
            major,
            minor,
            patch,
        }
    }
}

//#[derive(Deserialize, Serialize, Debug)]
//pub struct FinishedJob;

#[derive(Deserialize, Serialize, Debug, Constructor)]
/// a file or directory loaded into memory to be saved after transport
pub struct SendFile {
    pub file_path: PathBuf,
    pub is_file: bool,
    pub bytes: Vec<u8>,
}

#[derive(Deserialize, Serialize, Debug, Constructor)]
/// a file (not a directory) that describes the save path and signals that the next message sent through the buffer will
/// be very large, and should be directly copied to a file IO stream instead of stored in memory
pub struct FileMarker {
    pub file_path: PathBuf,
}

fn serialization_options() -> bincode::config::DefaultOptions {
    bincode::config::DefaultOptions::new()
}

#[derive(derive_more::From)]
pub(crate) struct Connection<T> {
    conn: TcpStream,
    _marker: std::marker::PhantomData<T>,
}

impl<T> fmt::Debug for Connection<T> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "connection transporting {} messages", stringify!(T))
    }
}

impl<TX, RX> Connection<TX>
where
    TX: Serialize + AssociatedMessage<Receive = RX>,
    RX: DeserializeOwned,
{
    pub(crate) async fn new(addr: SocketAddr) -> Result<Self, error::TcpConnection> {
        let conn = TcpStream::connect(addr)
            .await
            .map_err(error::TcpConnection::from)?;

        Ok(Self {
            conn,
            _marker: std::marker::PhantomData,
        })
    }

    pub(crate) fn from_connection(conn: TcpStream) -> Self {
        Self {
            conn,
            _marker: std::marker::PhantomData,
        }
    }

    pub(crate) async fn transport_data(
        &mut self,
        request: &TX,
    ) -> Result<(), error::TcpConnection> {
        transport(
            &mut self.conn,
            request,
            <TX as AssociatedMessage>::IS_KEEPALIVE,
        )
        .await
    }

    pub(crate) async fn transport_from_reader<R: AsyncRead + Unpin>(
        &mut self,
        reader: R,
        len: u64,
    ) -> Result<(), error::TcpConnection> {
        transport_from_reader(&mut self.conn, reader, len).await
    }

    pub(crate) async fn receive_data(&mut self) -> Result<RX, error::TcpConnection> {
        receive(&mut self.conn, <TX as AssociatedMessage>::IS_KEEPALIVE).await
    }

    pub(crate) async fn receive_to_writer<W: AsyncWrite + Unpin>(
        &mut self,
        writer: W,
    ) -> Result<(), error::TcpConnection> {
        receive_to_writer(&mut self.conn, writer).await
    }

    pub(crate) async fn reconnect(&mut self, addr: &SocketAddr) -> Result<(), Error> {
        let conn = TcpStream::connect(&addr)
            .await
            .map_err(error::TcpConnection::from)?;
        self.conn = conn;

        Ok(())
    }

    pub(crate) fn update_state<TxNew>(self) -> Connection<TxNew> {
        let conn = self.conn;
        Connection {
            conn,
            _marker: std::marker::PhantomData,
        }
    }

    #[cfg(test)]
    /// determine how many bytes are remaining in the connection
    pub(crate) async fn bytes_left(&mut self) -> usize {
        let mut buffer = vec![0; 100];
        //let fut = read_buffer_bytes(&mut buffer, &mut self.conn);
        let fut = self.conn.read(&mut buffer[0..]);
        let buffer_length: Result<Result<usize, _>, _> =
            tokio::time::timeout(Duration::from_millis(500), fut).await;
        buffer_length.map(|x| x.unwrap_or(0)).unwrap_or(0)
    }
}

/// serialize a `T` to bytes and send it through a TCP connection
async fn transport<T: Serialize>(
    tcp_connection: &mut TcpStream,
    data: &T,
    is_keepalive: bool,
) -> Result<(), error::TcpConnection> {
    let serializer = serialization_options();
    let bytes = serializer
        .serialize(&data)
        .map_err(error::Serialization::from)?;

    if is_keepalive {
        debug!(
            "keepalive - sending buffer of length {} to {:?}",
            bytes.len(),
            tcp_connection.peer_addr()
        );
    } else {
        debug!("sending buffer of length {}", bytes.len());
    }

    let bytes_len: u64 = bytes.len() as u64;

    // write the length of the data that we are first sending
    tcp_connection.write_all(&bytes_len.to_le_bytes()).await?;

    // write the contents of the actual data now that the length of the data is
    // actually known
    tcp_connection.write_all(&bytes).await?;

    Ok(())
}

/// send raw bytes from a [`AsyncRead`]er through a [`TcpStream`]
async fn transport_from_reader<R: AsyncReadExt + Unpin>(
    conn: &mut TcpStream,
    mut reader: R,
    length: u64,
) -> Result<(), error::TcpConnection> {
    debug!("transporting extra large with with length {}", length);
    conn.write_all(&length.to_le_bytes()).await?;

    let mut buffer = vec![0; BUFFER_LEN];
    let mut total_bytes = 0;

    loop {
        let bytes_read = reader.read(&mut buffer).await?;

        conn.write_all(&buffer[0..bytes_read]).await?;

        total_bytes += bytes_read;

        trace!(
            "while transporting file, read {} ({} / {}) bytes from the reader",
            bytes_read,
            total_bytes,
            length
        );

        #[cfg(test)]
        if bytes_read == 0 {
            panic!("read 0 bytes in a test");
        }

        if total_bytes == length as usize {
            break;
        }
    }

    Ok(())
}

/// use `bincode` to receive and deserialize a type from a connection
async fn receive<T: DeserializeOwned>(
    tcp_connection: &mut TcpStream,
    is_keepalive: bool,
) -> Result<T, error::TcpConnection> {
    let deserializer = serialization_options();

    let mut buf: [u8; 8] = [0; 8];

    read_buffer_bytes(&mut buf, tcp_connection).await?;

    let content_length = u64::from_le_bytes(buf);

    if is_keepalive {
        debug!(
            "keepalive - receiving buffer with length {} to {:?}",
            content_length,
            tcp_connection.peer_addr()
        );
    } else {
        debug!("receiving buffer with length {}", content_length);
    }

    let mut content_buffer = vec![0; content_length as usize];

    read_buffer_bytes(&mut content_buffer, tcp_connection).await?;

    let output = deserializer
        .deserialize(&content_buffer)
        .map_err(error::Deserialization::from)?;

    Ok(output)
}

/// read a raw byte stream from a tcp connection to a reader
///
/// does **not** work for zero sized types!
async fn receive_to_writer<W: AsyncWrite + Unpin>(
    conn: &mut TcpStream,
    mut writer: W,
) -> Result<(), error::TcpConnection> {
    let mut buf: [u8; 8] = [0; 8];

    read_buffer_bytes(&mut buf, conn).await?;

    let content_length = u64::from_le_bytes(buf);

    debug!(
        "receiving extra large buffer with length {}",
        content_length
    );

    let mut tmp_buffer = vec![0; BUFFER_LEN];

    let mut running_length = 0;

    loop {
        let bytes_read = conn
            .read(&mut tmp_buffer)
            .await
            .map_err(error::TcpConnection::from)?;

        trace!(
            "read {} bytes ({} / {}) from TCP connection to copy to a writer",
            bytes_read,
            running_length,
            content_length
        );

        // this should mean that the other party has closed the connection
        if bytes_read == 0 {
            return Err(error::TcpConnection::ConnectionClosed);
        }

        // just ignore any errors in the writing process
        // TODO: do not ignore the errors here
        writer.write_all(&tmp_buffer[0..bytes_read]).await?;

        running_length += bytes_read;

        if running_length as u64 == content_length {
            break;
        }
    }

    Ok(())
}

/// a generic and useful function for pulling a fixed number of bytes from a [`TcpStream`]
///
/// does **not** work for zero sized types!
async fn read_buffer_bytes(
    buffer: &mut [u8],
    conn: &mut TcpStream,
) -> Result<(), error::TcpConnection> {
    let mut starting_idx = 0;

    loop {
        let bytes_read = conn
            .read(&mut buffer[starting_idx..])
            .await
            .map_err(error::TcpConnection::from)?;

        // this should mean that the other party has closed the connection
        if bytes_read == 0 {
            return Err(error::TcpConnection::ConnectionClosed);
        }

        starting_idx += bytes_read;

        if starting_idx == buffer.len() {
            break;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::convert::TryInto;
    use std::net::SocketAddr;
    use std::time::{Duration, Instant};
    use tokio::net::TcpListener;

    #[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
    struct Arbitrary {
        bytes: Vec<u8>,
    }

    impl AssociatedMessage for Arbitrary {
        type Receive = Arbitrary;
    }

    type Connection = super::Connection<Arbitrary>;

    fn add_port(port: u16) -> SocketAddr {
        SocketAddr::from(([0, 0, 0, 0], port))
    }

    /// make sure that if we are just waiting on more bytes it doesnt
    /// mean that we have hit an EOF (that the other connection
    #[tokio::test]
    async fn ensure_not_eof() {
        let client_listener = TcpListener::bind(add_port(9994)).await.unwrap();
        let mut raw_server_connection = TcpStream::connect(add_port(9994)).await.unwrap();
        // connection from the client
        let mut client_connection =
            Connection::from_connection(client_listener.accept().await.unwrap().0);

        let bytes = (0..255).into_iter().collect::<Vec<u8>>();
        let payload = Arbitrary { bytes };

        // serialize the data manually
        let serializer = serialization_options();
        let server_req_bytes: Vec<u8> = serializer.serialize(&payload).unwrap();

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
        assert_eq!(client_version_of_request.unwrap(), payload);
    }

    /// make sure that if we close the connection an EOF is reached
    #[tokio::test]
    async fn ensure_eof_on_close() {
        let client_listener = TcpListener::bind(add_port(9995)).await.unwrap();
        let addr = add_port(9995);
        let server_connection = Connection::new(addr).await;

        let mut client_connection =
            Connection::from_connection(client_listener.accept().await.unwrap().0);

        std::mem::drop(server_connection);

        let rx = client_connection.receive_data().await;

        // ensure its `error::TcpConnection::ConnectionClosed`
        match rx.unwrap_err() {
            error::TcpConnection::ConnectionClosed => {
                // we are good
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
        let mut server_connection = Connection::new(addr).await.unwrap();

        let _client_connection =
            Connection::from_connection(client_listener.accept().await.unwrap().0);

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

    #[test]
    fn version_parses() {
        let v = Version::current_version();
        assert_eq!(v.to_string(), env!("CARGO_PKG_VERSION"));
    }

    #[test]
    #[serial_test::serial]
    /// ensure the length of data in the filesystem is the length of the
    /// data if we were to read it into a vector
    fn check_filesystem_bytes() {
        let length = 40732;
        let buffer = vec![0; length];
        let path = "./tests/buffer_check.binary";
        dbg!(std::env::current_dir().unwrap());

        let mut f = std::fs::File::create(&path).unwrap();

        f.write_all(&buffer).unwrap();
        std::mem::drop(f);

        let len = std::fs::metadata(&path).unwrap().len();

        std::fs::remove_file(path).ok();
        assert_eq!(length as u64, len);
    }

    #[tokio::test]
    async fn read_from_writer() {
        let len = 1099;
        let reader = std::io::Cursor::new(vec![0u8; len]);

        let server_port = 9999;

        let server_listener = TcpListener::bind(add_port(server_port)).await.unwrap();
        let mut client_conn = TcpStream::connect(add_port(server_port)).await.unwrap();
        let mut server_conn = server_listener.accept().await.unwrap().0;

        let mut received_data = Vec::<u8>::new();

        transport_from_reader(&mut client_conn, reader, len as u64)
            .await
            .unwrap();
        receive_to_writer(&mut server_conn, &mut received_data)
            .await
            .unwrap();

        assert!(received_data.len() == len);
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn read_from_writer_binary() {
        //crate::logger();
        let len = 1099;
        let path = PathBuf::from("./tests/read_from_writer_file.binary");
        std::fs::File::create(&path)
            .unwrap()
            .write_all(&vec![0u8; len])
            .unwrap();
        let reader = tokio::fs::File::open(&path).await.unwrap();

        let server_port = 10_002;

        let server_listener = TcpListener::bind(add_port(server_port)).await.unwrap();
        let mut client_conn = TcpStream::connect(add_port(server_port)).await.unwrap();
        let mut server_conn = server_listener.accept().await.unwrap().0;

        let mut received_data = Vec::<u8>::new();

        transport_from_reader(&mut client_conn, reader, len as u64)
            .await
            .unwrap();
        receive_to_writer(&mut server_conn, &mut received_data)
            .await
            .unwrap();
        std::fs::remove_file(&path).ok();

        assert!(received_data.len() == len);
        //panic!()
    }
}
