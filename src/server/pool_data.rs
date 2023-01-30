use super::ok_if_exists;
use super::schedule::JobSetIdentifier;
use crate::config::requirements::{NodeProvidedCaps, Requirements};

use crate::prelude::*;

#[derive(Debug, PartialEq)]
pub(crate) enum JobResponse {
    SetupOrRun(TaskInfo),
    EmptyJobs,
}

#[derive(derive_more::From)]
pub(crate) enum JobRequest {
    NewJob(NewJobRequest),
    /// a client failed a keepalive check while it was
    /// executing
    DeadNode(RunTaskInfo),
    AddJobSet(config::Jobs<config::common::HashedFile>),
    QueryRemainingJobs(RemainingJobsQuery),
    CancelBatchByName(CancelBatchQuery),
    MarkBuildFailure(MarkBuildFailure),
    FinishJob(FinishJob),
}

#[derive(PartialEq)]
pub(crate) struct FinishJob {
    pub(crate) ident: JobSetIdentifier,
    pub(crate) job_name: String,
}

pub(crate) struct MarkBuildFailure {
    pub(crate) ident: JobSetIdentifier,
}

pub(crate) struct NewJobRequest {
    pub(crate) tx: oneshot::Sender<JobResponse>,
    pub(crate) initialized_job: JobSetIdentifier,
    pub(crate) capabilities: Arc<Requirements<NodeProvidedCaps>>,
    pub(crate) build_failures: BTreeSet<JobSetIdentifier>,
    pub(crate) node_meta: NodeMetadata,
}

#[derive(derive_more::Constructor)]
pub(crate) struct RemainingJobsQuery {
    pub tx: oneshot::Sender<Vec<super::schedule::RemainingJobs>>,
}

#[derive(derive_more::Constructor)]
pub(crate) struct CancelBatchQuery {
    pub(crate) cancel_batch: oneshot::Sender<CancelResult>,
    pub(crate) batch_name: String,
}

#[derive(Display, Serialize, Deserialize, Debug, Clone)]
pub enum CancelResult {
    #[display(fmt = "Batch name was missing")]
    BatchNameMissing,
    #[display(fmt = "There were no nodes to broadcast to")]
    NoBroadcastNodes,
    #[display(fmt = "success")]
    Success,
}

#[derive(Clone)]
pub(crate) struct PendingJob {
    task: JobOrInit,
    ident: JobSetIdentifier,
}

#[derive(From, Clone, Constructor, Debug)]
pub(crate) struct TaskInfo {
    namespace: String,
    batch_name: String,
    pub(crate) identifier: JobSetIdentifier,
    pub(crate) task: JobOrInit,
}

impl PartialEq for TaskInfo {
    fn eq(&self, other: &Self) -> bool {
        self.namespace == other.namespace
            && self.batch_name == other.batch_name
            && self.identifier == other.identifier
    }
}

impl TaskInfo {
    pub(crate) fn flatten(self) -> FetchedJob {
        let TaskInfo {
            namespace,
            batch_name,
            identifier,
            task,
        } = self;

        match task {
            JobOrInit::Job(task) => RunTaskInfo {
                namespace,
                batch_name,
                identifier,
                task,
            }
            .into(),
            JobOrInit::JobInit(init) => BuildTaskInfo {
                namespace,
                batch_name,
                identifier,
                init,
            }
            .into(),
        }
    }
}

#[derive(From)]
pub(crate) enum FetchedJob {
    Build(BuildTaskInfo),
    Run(RunTaskInfo),
    MissedKeepalive,
}

#[derive(From, Clone, Constructor, Serialize, Deserialize)]
pub(crate) struct BuildTaskInfo {
    pub(crate) namespace: String,
    pub(crate) batch_name: String,
    pub(crate) identifier: JobSetIdentifier,
    pub(crate) init: config::Init,
}

impl BuildTaskInfo {
    pub(crate) async fn batch_save_path(
        &self,
        base_path: &Path,
    ) -> Result<PathBuf, (std::io::Error, PathBuf)> {
        let path = base_path.join(&self.namespace).join(&self.batch_name);

        debug!("creating path {} for build", path.display());
        ok_if_exists(tokio::fs::create_dir_all(&path).await).map_err(|e| (e, path.clone()))?;

        Ok(path)
    }
}

#[derive(From, Clone, Constructor, Serialize, Deserialize)]
pub(crate) struct RunTaskInfo {
    pub(crate) namespace: String,
    pub(crate) batch_name: String,
    pub(crate) identifier: JobSetIdentifier,
    pub(crate) task: config::Job,
}

#[cfg(test)]
impl RunTaskInfo {
    pub(crate) fn placeholder_data() -> Self {
        RunTaskInfo {
            namespace: "some_namespace".into(),
            batch_name: "some_batch".into(),
            identifier: JobSetIdentifier::Identity(1),
            task: config::Job::placeholder_apptainer(),
        }
    }
}

#[cfg_attr(test, derive(derive_more::Unwrap))]
#[derive(From, Clone, Debug)]
pub(crate) enum JobOrInit {
    Job(config::Job),
    JobInit(config::Init),
}

#[derive(Display, Clone, Debug, Serialize, Deserialize, Constructor)]
#[display(fmt = "{node_name} : {node_address}")]
/// information about the compute node that is stored on the scheduling server.
///
/// This is a nice wrapper that is shared in different places. It contains the
/// name of the node (as defined in the .yaml config file for the node) and the
/// main transport address used to reach the node.
pub(crate) struct NodeMetadata {
    pub(crate) node_name: String,
    pub(crate) node_address: SocketAddr,
}

#[cfg(test)]
impl NodeMetadata {
    pub(crate) fn by_name(name: &str) -> Self {
        Self {
            node_name: name.to_owned(),
            node_address: ([0, 0, 0, 0], 0).into(),
        }
    }

    pub(crate) fn test_name() -> Self {
        Self {
            node_name: "Test Name".to_owned(),
            node_address: ([0, 0, 0, 0], 0).into(),
        }
    }
}
