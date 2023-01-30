//! Lays out the flow of how jobs are executed between a server and the client
//!
//! See SVG graphic in general user documentation for a flowchart
//!
//! [`uninit::Uninit`] can move to [`prepare_build::PrepareBuild`]
//!
//! [`prepare_build::PrepareBuild`] can move to [`compiling::Building`]
//!
//! [`compiling::Building`] can move to [`built::Built`] (in the case that everything compiled
//! correctly and we await jobs to execute) or [`prepare_build::PrepareBuild`] (in the case that
//! compilation failed or the compilation was cancelled)
//!
//! [`built::Built`] can move to [`executing::Executing`] (if a job was assigned to the node) or
//! [`prepare_build::PrepareBuild`] (if the job was cancelled)
//!
//! [`executing::Executing`] can move to [`send_files::SendFiles`] (if the job finished, transport
//! all results to head node)
//!
//! [`send_files::SendFiles`] can move (back) to [`built::Built`] after all files have been
//! transported
//!
//! [`prepare_build::PrepareBuild`], [`compiling::Building`], [`built::Built`], [`executing::Executing`], [`send_files::SendFiles`] can all return to
//! [`uninit::Uninit`] if the connection is dropped
///
///
/// ## Cancellations
///
/// We do not need to worry about cancellation messages for the most part. This is because
/// cancellation messages on the server node task have a type
/// `tokio::sync::broadcast::Receiver<JobSetIdentifier>`, so reading through the cancellations later
/// will still give us identical information as we have a full history of whats being cancelled.
/// For example, we dont need to worry about cancellations in the compiling phase because once the
/// job is compiled, we will request jobs from the server and there will be none (as they were all
/// removed when the job was cancelled).
///
/// The only place we *do* need to worry about cancellations are in the execution phase.
use crate::prelude::*;
use crate::server::pool_data;
use crate::server::JobSetIdentifier;
use std::collections::BTreeSet;
use tokio::sync::broadcast;

#[macro_export]
#[doc(hidden)]
/// match on the given $result, and if the $result is Err(e),
/// return Err((self, x))
///
/// this is because the current borrow checker does not like
/// us using .map_err() to combine with `self`, even though
/// we are using `?` immediately after
macro_rules! throw_error_with_self {
    ($result:expr, $_self:expr) => {
        match $result {
            Ok(x) => x,
            Err(e) => {
                error!("just threw with error");
                return Err(($_self, e.into()));
            }
        }
    };
}

pub(crate) mod built;
pub(crate) mod compiling;
pub(crate) mod executing;
pub(crate) mod prepare_build;
pub(crate) mod send_files;
pub(crate) mod uninit;

pub(crate) type UninitClient = Machine<uninit::Uninit, uninit::ClientUninitState>;
pub(crate) type PrepareBuildClient =
    Machine<prepare_build::PrepareBuild, prepare_build::ClientPrepareBuildState>;
pub(crate) type BuildingClient = Machine<compiling::Building, compiling::ClientBuildingState>;
pub(crate) type BuiltClient = Machine<built::Built, built::ClientBuiltState>;
pub(crate) type ExecuteClient = Machine<executing::Executing, executing::ClientExecutingState>;
pub(crate) type SendFilesClient<T> = Machine<send_files::SendFiles, send_files::SenderState<T>>;

pub(crate) type UninitServer = Machine<uninit::Uninit, uninit::ServerUninitState>;
pub(crate) type PrepareBuildServer =
    Machine<prepare_build::PrepareBuild, prepare_build::ServerPrepareBuildState>;
pub(crate) type BuildingServer = Machine<compiling::Building, compiling::ServerBuildingState>;
pub(crate) type BuiltServer = Machine<built::Built, built::ServerBuiltState>;
pub(crate) type ExecuteServer = Machine<executing::Executing, executing::ServerExecutingState>;
pub(crate) type SendFilesServer<T> = Machine<send_files::SendFiles, send_files::ReceiverState<T>>;

#[derive(Debug)]
pub(crate) struct Machine<StateMarker, State> {
    _marker: StateMarker,
    state: State,
}

impl<StateMarker, State> Machine<StateMarker, State>
where
    StateMarker: Default,
{
    pub(crate) fn from_state(state: State) -> Self {
        Self {
            state,
            _marker: StateMarker::default(),
        }
    }
}

impl<StateMarker, T> Machine<StateMarker, transport::Connection<T>> {
    pub(crate) fn into_inner(self) -> transport::Connection<T> {
        self.state
    }
}

impl<T> SendFilesServer<T> {
    pub(crate) fn into_connection(self) -> transport::Connection<send_files::ServerMsg> {
        self.state.conn
    }
}

pub(crate) enum Either<T, V> {
    Left(T),
    Right(V),
}

#[derive(From, thiserror::Error, Debug)]
pub(crate) enum ClientError {
    #[error("error in uninit state: `{0}`")]
    Uninit(uninit::ClientError),
    #[error("error in prepare build state: `{0}`")]
    PrepareBuild(prepare_build::ClientError),
    #[error("error in compiling state: `{0}`")]
    Building(compiling::ClientError),
    #[error("error in built state: `{0}`")]
    Built(built::ClientError),
    #[error("error in executing state: `{0}`")]
    Executing(executing::ClientError),
    #[error("error in send files state: `{0}`")]
    SendFiles(send_files::ClientError),
    #[error("error in send files state: `{0}`")]
    ReceiveFiles(send_files::ServerError),
}

impl ClientError {
    pub(crate) fn is_tcp_error(&self) -> bool {
        matches!(
            &self,
            Self::Uninit(uninit::ClientError::TcpConnection(_))
                | Self::PrepareBuild(prepare_build::ClientError::TcpConnection(_))
                | Self::Building(compiling::ClientError::TcpConnection(_))
                | Self::Built(built::ClientError::TcpConnection(_))
                | Self::Executing(executing::ClientError::TcpConnection(_))
                | Self::SendFiles(send_files::ClientError::TcpConnection(_))
                | Self::ReceiveFiles(send_files::ServerError::TcpConnection(_))
        )
    }
}

#[derive(From, thiserror::Error, Debug)]
pub(crate) enum ServerError {
    #[error("error in uninit state: `{0}`")]
    Uninit(uninit::ServerError),
    #[error("error in prepare_build state: `{0}`")]
    PrepareBuild(prepare_build::ServerError),
    #[error("error in compiling state: `{0}`")]
    Building(compiling::ServerError),
    #[error("error in built state: `{0}`")]
    Built(built::ServerError),
    #[error("error in executing state: `{0}`")]
    Executing(executing::ServerError),
    #[error("error in send files state: `{0}`")]
    ReceiveFiles(send_files::ServerError),
    #[error("error in send files state: `{0}`")]
    SendFiles(send_files::ClientError),
}

impl ServerError {
    pub(crate) fn is_tcp_error(&self) -> bool {
        matches!(
            &self,
            Self::Uninit(uninit::ServerError::TcpConnection(_))
                | Self::PrepareBuild(prepare_build::ServerError::TcpConnection(_))
                | Self::Building(compiling::ServerError::TcpConnection(_))
                | Self::Built(built::ServerError::TcpConnection(_))
                | Self::Executing(executing::ServerError::TcpConnection(_))
                | Self::ReceiveFiles(send_files::ServerError::TcpConnection(_))
                | Self::SendFiles(send_files::ClientError::TcpConnection(_))
        )
    }
}
type ClientEitherPrepareBuild<T> =
    Either<Machine<prepare_build::PrepareBuild, prepare_build::ClientPrepareBuildState>, T>;
type ServerEitherPrepareBuild<T> =
    Either<Machine<prepare_build::PrepareBuild, prepare_build::ServerPrepareBuildState>, T>;

#[derive(Constructor, Debug)]
pub(crate) struct Common {
    receive_cancellation: broadcast::Receiver<JobSetIdentifier>,
    capabilities: Arc<Requirements<NodeProvidedCaps>>,
    /// the root directory on the HDD that we should save results of the runs,
    /// not including namespace and batch name information
    save_path: PathBuf,
    pub(crate) node_meta: pool_data::NodeMetadata,
    keepalive_addr: SocketAddr,
    pub(crate) main_transport_addr: SocketAddr,
    /// address on the compute node to message if the job was
    /// cancelled
    pub(crate) cancel_addr: SocketAddr,
    errored_jobs: BTreeSet<server::JobSetIdentifier>,
}

impl Common {
    #[cfg(test)]
    fn test_configuration(
        transport_addr: SocketAddr,
        keepalive_addr: SocketAddr,
        cancel_addr: SocketAddr,
    ) -> (broadcast::Sender<JobSetIdentifier>, Self) {
        let (tx, rx) = broadcast::channel(1);

        let capabilities = Arc::new(vec![].into_iter().collect());
        let save_path = PathBuf::from("./tests/unittests");
        let node_meta = server::pool_data::NodeMetadata::test_name();
        let errored_jobs = Default::default();

        // try to make the save path
        std::fs::create_dir(&save_path).ok();

        let common = Common::new(
            rx,
            capabilities,
            save_path,
            node_meta,
            keepalive_addr,
            transport_addr,
            cancel_addr,
            errored_jobs,
        );

        // tx here is a cancellation handle
        (tx, common)
    }
}
