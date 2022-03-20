use crate::prelude::*;
use crate::server::JobIdentifier;
use tokio::sync::broadcast;
use std::collections::BTreeSet;

mod executing;
mod prepare_build;
mod uninit;
mod compiling;
mod built;

pub(crate) struct Machine<StateMarker, State> {
    _marker: StateMarker,
    state: State,
}

pub(crate) enum Either<T,V> {
    Left(T),
    Right(V),
}

type ClientEitherPrepareBuild<T> = Either<Machine<prepare_build::PrepareBuild, prepare_build::ClientPrepareBuildState>, T>;
type ServerEitherPrepareBuild<T> = Either<Machine<prepare_build::PrepareBuild, prepare_build::ServerPrepareBuildState>, T>;

#[derive(Constructor)]
// copy pasted from node.rs
pub(crate) struct Common {
    receive_cancellation: broadcast::Receiver<JobIdentifier>,
    capabilities: Arc<Requirements<NodeProvidedCaps>>,
    save_path: PathBuf,
    node_name: String,
    keepalive_addr: SocketAddr,
    errored_jobs: BTreeSet<server::JobIdentifier>
}

mod send_files {
    pub(crate) struct SendFiles;
    pub(crate) struct ClientSendFilesState;
    pub(crate) struct ServerSendFilesState;
}
