use super::ok_if_exists;
use super::schedule::{JobIdentifier, NodeProvidedCaps, Requirements};
use super::storage;
use crate::config;

use std::collections::BTreeSet;

use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::oneshot;

use derive_more::{Constructor, Display, From};

use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq)]
pub(crate) enum JobResponse {
    SetupOrRun(TaskInfo),
    EmptyJobs,
}

#[derive(derive_more::From)]
pub(crate) enum JobRequest {
    NewJob(NewJobRequest),
    DeadNode(RunTaskInfo),
    AddJobSet(storage::OwnedJobSet),
    QueryRemainingJobs(RemainingJobsQuery),
    CancelBatchByName(CancelBatchQuery),
    MarkBuildFailure(MarkBuildFailure),
    FinishJob(JobIdentifier),
}

pub(crate) struct MarkBuildFailure {
    pub(crate) ident: JobIdentifier,
}

pub(crate) struct NewJobRequest {
    pub(crate) tx: oneshot::Sender<JobResponse>,
    pub(crate) initialized_job: JobIdentifier,
    pub(crate) capabilities: Arc<Requirements<NodeProvidedCaps>>,
    pub(crate) build_failures: BTreeSet<JobIdentifier>,
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
    ident: JobIdentifier,
}

#[derive(From, Clone, Constructor, Debug, PartialEq)]
pub(crate) struct TaskInfo {
    namespace: String,
    batch_name: String,
    pub(crate) identifier: JobIdentifier,
    pub(crate) task: JobOrInit,
}

impl TaskInfo {
    pub(crate) fn flatten(self) -> BuildTaskRunTask {
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
            JobOrInit::JobInit(task) => BuildTaskInfo {
                namespace,
                batch_name,
                identifier,
                task,
            }
            .into(),
        }
    }
}

#[derive(From)]
pub(crate) enum BuildTaskRunTask {
    Build(BuildTaskInfo),
    Run(RunTaskInfo),
}

#[derive(From, Clone, Constructor)]
pub(crate) struct BuildTaskInfo {
    namespace: String,
    batch_name: String,
    pub(crate) identifier: JobIdentifier,
    pub(crate) task: config::BuildOpts,
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

#[derive(From, Clone, Constructor)]
pub(crate) struct RunTaskInfo {
    namespace: String,
    batch_name: String,
    pub(crate) identifier: JobIdentifier,
    pub(crate) task: storage::JobOpt,
}

impl RunTaskInfo {
    pub(crate) async fn batch_save_path(
        &self,
        base_path: &Path,
    ) -> Result<PathBuf, (std::io::Error, PathBuf)> {
        let path = base_path
            .join(&self.namespace)
            .join(&self.batch_name)
            .join(self.task.name());

        debug!("creating path {} for job", path.display());
        // TODO: clear the contents of the folder if it already exists
        ok_if_exists(tokio::fs::create_dir_all(&path).await).map_err(|e| (e, path.clone()))?;

        Ok(path)
    }
}

#[cfg_attr(test, derive(derive_more::Unwrap))]
#[derive(From, Clone, Debug, PartialEq)]
pub(crate) enum JobOrInit {
    Job(storage::JobOpt),
    JobInit(config::BuildOpts),
}
