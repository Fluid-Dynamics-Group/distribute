use crate::prelude::*;
use crate::server::JobIdentifier;
use std::collections::BTreeSet;
use tokio::sync::broadcast;

mod built;
mod compiling;
mod executing;
mod prepare_build;
mod send_files;
mod uninit;

pub(crate) struct Machine<StateMarker, State> {
    _marker: StateMarker,
    state: State,
}

pub(crate) enum Either<T, V> {
    Left(T),
    Right(V),
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
