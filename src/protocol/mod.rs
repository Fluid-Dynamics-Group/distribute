use crate::prelude::*;
use crate::server::JobIdentifier;
use std::collections::BTreeSet;
use tokio::sync::broadcast;

#[macro_export]
#[doc = "hidden"]
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

pub(crate) struct Machine<StateMarker, State> {
    _marker: StateMarker,
    state: State,
}

pub(crate) enum Either<T, V> {
    Left(T),
    Right(V),
}

pub(crate) enum Error {}

//#[derive(Constructor)]
//pub(crate) struct TcpErrorWrap {
//    error: error::TcpConnection,
//    module: Module,
//}
//
//enum Module {
//    Built,
//    Compiling,
//    Executing,
//    PrepareBuild,
//    SendFiles,
//    Uninit,
//}

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
