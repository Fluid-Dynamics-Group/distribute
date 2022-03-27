use crate::prelude::*;
use crate::server::JobIdentifier;
use std::collections::BTreeSet;
use tokio::sync::broadcast;

#[macro_export]
#[doc(hidden)]
macro_rules! throw_error_with_self {
    ($result:expr, $_self:expr) => {
        match $result {
            Ok(x) => x,
            Err(e) => return Err(($_self, e.into())),
        }
    };
}

pub(crate) mod built;
pub(crate) mod compiling;
pub(crate) mod executing;
pub(crate) mod prepare_build;
pub(crate) mod send_files;
pub(crate) mod uninit;

pub(crate) type UninitClient =
    Machine<uninit::Uninit, uninit::ClientUninitState>;
pub(crate) type UninitServer =
    Machine<uninit::Uninit, uninit::ServerUninitState>;
pub(crate) type PrepareBuildClient =
    Machine<prepare_build::PrepareBuild, prepare_build::ClientPrepareBuildState>;
pub(crate) type BuiltClient=
    Machine<built::Built, built::ClientBuiltState>;
pub(crate) type ExecuteClient=
    Machine<executing::Executing, executing::ClientExecutingState>;
pub(crate) type SendFilesClient=
    Machine<executing::Executing, executing::ClientExecutingState>;

pub(crate) struct Machine<StateMarker, State> {
    _marker: StateMarker,
    state: State,
}

pub(crate) enum Either<T, V> {
    Left(T),
    Right(V),
}


#[derive(From)]
pub(crate) enum ClientError {
    Uninit(uninit::ClientError),
    PrepareBuild(prepare_build::ClientError),
    Building(compiling::ClientError),
    Built(built::ClientError),
    Executing(executing::ClientError),
    SendFiles(send_files::ClientError),
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
    save_path: PathBuf,
    node_name: String,
    keepalive_addr: SocketAddr,
    main_transport_addr: SocketAddr,
    errored_jobs: BTreeSet<server::JobIdentifier>,
}
