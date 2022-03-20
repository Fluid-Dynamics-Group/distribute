use crate::prelude::*;
use crate::server::JobIdentifier;
use tokio::sync::broadcast;
use std::collections::BTreeSet;

mod prepare_build;
mod uninit;
mod compiling;

pub(crate) struct Machine<StateMarker, State> {
    _marker: StateMarker,
    state: State,
}

pub(crate) enum Either<T,V> {
    Left(T),
    Right(V),
}

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



mod built {
    pub(crate) struct Built;
    pub(crate) struct ClientBuiltState;
    pub(crate) struct ServerBuiltState;
}

pub(crate) struct Executing;
pub(crate) struct SendFiles;
