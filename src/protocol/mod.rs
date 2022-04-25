use crate::prelude::*;
use crate::server::JobIdentifier;
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
                debug!("just threw with error");
                return Err(($_self, e.into()))
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
pub(crate) type BuiltClient = Machine<built::Built, built::ClientBuiltState>;
pub(crate) type ExecuteClient = Machine<executing::Executing, executing::ClientExecutingState>;
pub(crate) type SendFilesClient = Machine<send_files::SendFiles, send_files::ClientSendFilesState>;

pub(crate) type UninitServer = Machine<uninit::Uninit, uninit::ServerUninitState>;
pub(crate) type PrepareBuildServer =
    Machine<prepare_build::PrepareBuild, prepare_build::ServerPrepareBuildState>;
pub(crate) type BuiltServer = Machine<built::Built, built::ServerBuiltState>;
pub(crate) type ExecuteServer = Machine<executing::Executing, executing::ServerExecutingState>;
pub(crate) type SendFilesServer = Machine<send_files::SendFiles, send_files::ServerSendFilesState>;

pub(crate) struct Machine<StateMarker, State> {
    _marker: StateMarker,
    state: State,
}

impl<StateMarker, State> Machine<StateMarker, State>
where
    StateMarker: Default,
{
    fn from_state(state: State) -> Self {
        Self {
            state,
            _marker: StateMarker::default(),
        }
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
}

impl ClientError {
    pub(crate) fn is_tcp_error(&self) -> bool {
        match &self {
            Self::Uninit(uninit::ClientError::TcpConnection(_)) => true,
            Self::PrepareBuild(prepare_build::ClientError::TcpConnection(_)) => true,
            Self::Building(compiling::ClientError::TcpConnection(_)) => true,
            Self::Built(built::ClientError::TcpConnection(_)) => true,
            Self::Executing(executing::ClientError::TcpConnection(_)) => true,
            Self::SendFiles(send_files::ClientError::TcpConnection(_)) => true,
            _ => false,
        }
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
    SendFiles(send_files::ServerError),
}

impl ServerError {
    pub(crate) fn is_tcp_error(&self) -> bool {
        match &self {
            Self::Uninit(uninit::ServerError::TcpConnection(_)) => true,
            Self::PrepareBuild(prepare_build::ServerError::TcpConnection(_)) => true,
            Self::Building(compiling::ServerError::TcpConnection(_)) => true,
            Self::Built(built::ServerError::TcpConnection(_)) => true,
            Self::Executing(executing::ServerError::TcpConnection(_)) => true,
            Self::SendFiles(send_files::ServerError::TcpConnection(_)) => true,
            _ => false,
        }
    }
}
type ClientEitherPrepareBuild<T> =
    Either<Machine<prepare_build::PrepareBuild, prepare_build::ClientPrepareBuildState>, T>;
type ServerEitherPrepareBuild<T> =
    Either<Machine<prepare_build::PrepareBuild, prepare_build::ServerPrepareBuildState>, T>;

#[derive(Constructor)]
// copy pasted from node.rs
pub(crate) struct Common {
    receive_cancellation: broadcast::Receiver<JobIdentifier>,
    capabilities: Arc<Requirements<NodeProvidedCaps>>,
    /// the root directory on the HDD that we should save results of the runs,
    /// not including namespace and batch name information
    save_path: PathBuf,
    pub(crate) node_name: String,
    keepalive_addr: SocketAddr,
    pub(crate) main_transport_addr: SocketAddr,
    errored_jobs: BTreeSet<server::JobIdentifier>,
}

impl Common {
    #[cfg(test)]
    fn test_configuration(
        transport_addr: SocketAddr,
        keepalive_addr: SocketAddr,
    ) -> (broadcast::Sender<JobIdentifier>, Self) {
        let (tx, rx) = broadcast::channel(1);

        let capabilities = Arc::new(vec![].into_iter().collect());
        let save_path = PathBuf::from("./tests/unittests");
        let node_name = "Test Name".into();
        let errored_jobs = Default::default();

        // try to make the save path
        std::fs::create_dir(&save_path).ok();

        let common = Common::new(
            rx,
            capabilities,
            save_path,
            node_name,
            keepalive_addr,
            transport_addr,
            errored_jobs,
        );

        (tx, common)
    }
}
