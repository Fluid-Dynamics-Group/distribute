use super::ok_if_exists;
use super::schedule::{self, JobIdentifier, NodeProvidedCaps, Requirements, Schedule};
use super::storage;
use crate::{cli, config, error, error::Error, status, transport};

use std::net::SocketAddr;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio::task::JoinHandle;

use derive_more::{Constructor, From, Display};

use serde::{Deserialize, Serialize};

#[derive(Constructor)]
pub(super) struct JobPool<T> {
    remaining_jobs: T,
    receive_requests: mpsc::Receiver<JobRequest>,
    broadcast_cancel: broadcast::Sender<JobIdentifier>,
}

impl<T> JobPool<T>
where
    T: Schedule + Send + 'static,
{
    pub(super) fn spawn(mut self) -> JoinHandle<()> {
        tokio::task::spawn(async move {
            while let Some(new_req) = self.receive_requests.recv().await {
                match new_req {
                    // we want a new job from the scheduler
                    JobRequest::NewJob(new_req) => {
                        debug!("a node has asked for a new job");
                        // if we are requesting a new job -not- right after building a job
                        if !new_req.after_building
                            && new_req.initialized_job != JobIdentifier::none()
                        {
                            info!("marking a finished job for {}", new_req.initialized_job);
                            self.remaining_jobs.finish_job(new_req.initialized_job);
                        }

                        let new_task: JobResponse = self
                            .remaining_jobs
                            .fetch_new_task(new_req.initialized_job, new_req.capabilities);
                        new_req.tx.send(new_task).ok().unwrap();
                    }
                    // a job failed to execute on the node
                    JobRequest::DeadNode(pending_job) => {
                        debug!("a node has died for now, the job is returning to the scheduler");

                        match pending_job.task {
                            JobOrInit::Job(job) => {
                                self.remaining_jobs
                                    .add_job_back(job, pending_job.identifier);
                            }
                            // an initialization job does not need to be returned to the
                            // scheduler
                            JobOrInit::JobInit(_init) => (),
                        };
                        continue;
                    }
                    // the server got a request to add a new job set
                    JobRequest::AddJobSet(set) => {
                        info!("added new job set `{}` to scheduler", set.batch_name);
                        if let Err(e) = self.remaining_jobs.insert_new_batch(set) {
                            error!("failed to insert now job set: {}", e);
                        }
                        // TODO: add pipe back to the main process so that we can
                        // alert the user if the job set was not added correctly
                        continue;
                    }
                    JobRequest::QueryRemainingJobs(responder) => {
                        let remaining_jobs = self.remaining_jobs.remaining_jobs();
                        responder
                            .tx
                            .send(remaining_jobs)
                            .map_err(|e| {
                                error!(
                                    "could not respond back to \
                                                the server task with information \
                                                on the remaining jobs: {:?}",
                                    e
                                )
                            })
                            .ok();
                    }
                    JobRequest::CancelBatchByName(cancel_query) => {
                        let identifier = self
                            .remaining_jobs
                            .identifier_by_name(&cancel_query.batch_name);

                        if let Some(found_identifier) = identifier {
                            if let Ok(_) = self.broadcast_cancel.send(found_identifier) {
                                debug!(
                                    "successfully sent cancellation message for batch name {}",
                                    &cancel_query.batch_name
                                );
                                cancel_query.cancel_batch.send(CancelResult::Success).ok();
                            } else {
                                cancel_query
                                    .cancel_batch
                                    .send(CancelResult::NoBroadcastNodes)
                                    .ok();
                                error!("cancellation broadcast has no receivers! This should only happen if there
                                       were no nodes initialized");
                            }
                        } else {
                            warn!("batch name {} was missing from the job set - unable to cancel the jobs", &cancel_query.batch_name);
                            cancel_query
                                .cancel_batch
                                .send(CancelResult::BatchNameMissing)
                                .ok();
                        }
                    }
                };
                //
            }
        })
    }
}

pub(crate) enum JobResponse {
    SetupOrRun(TaskInfo),
    EmptyJobs,
}

#[derive(derive_more::From)]
pub(crate) enum JobRequest {
    NewJob(NewJobRequest),
    DeadNode(TaskInfo),
    AddJobSet(storage::OwnedJobSet),
    QueryRemainingJobs(RemainingJobsQuery),
    CancelBatchByName(CancelBatchQuery),
}

pub(crate) struct NewJobRequest {
    tx: oneshot::Sender<JobResponse>,
    initialized_job: JobIdentifier,
    after_building: bool,
    capabilities: Arc<Requirements<NodeProvidedCaps>>,
}

#[derive(derive_more::Constructor)]
pub(crate) struct RemainingJobsQuery {
    pub tx: oneshot::Sender<Vec<super::schedule::RemainingJobs>>,
}

#[derive(derive_more::Constructor)]
pub(crate) struct CancelBatchQuery {
    cancel_batch: oneshot::Sender<CancelResult>,
    batch_name: String,
}

#[derive(Display, Serialize, Deserialize, Debug, Clone)]
pub(crate) enum CancelResult {
    #[display(fmt="Batch name was missing")]
    BatchNameMissing,
    #[display(fmt="There were no nodes to broadcast to")]
    NoBroadcastNodes,
    #[display(fmt="success")]
    Success,
}

#[derive(Clone)]
pub(crate) struct PendingJob {
    task: JobOrInit,
    ident: JobIdentifier,
}

#[derive(From, Clone, Constructor)]
pub(crate) struct TaskInfo {
    namespace: String,
    batch_name: String,
    pub(crate) identifier: JobIdentifier,
    pub(crate) task: JobOrInit,
}

impl TaskInfo {
    fn flatten(self) -> BuildTaskRunTask {
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
enum BuildTaskRunTask {
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
    async fn batch_save_path(
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
    async fn batch_save_path(
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
#[derive(From, Clone)]
pub(crate) enum JobOrInit {
    Job(storage::JobOpt),
    JobInit(config::BuildOpts),
}

#[derive(derive_more::Constructor)]
pub(super) struct InitializedNode {
    common: Common,
}

impl InitializedNode {
    pub(super) fn spawn(mut self) -> JoinHandle<()> {
        tokio::task::spawn(async move {
            let mut last_set = None;
            // now we just pull jobs from the server until we are done
            loop {
                // if we are at this point in the loop then we need to build
                // a new job and execute it until there are no more remaining jobs
                match self.execute_job_set(last_set.take()).await {
                    Ok(new_set) => {
                        last_set = Some(new_set);
                        continue;
                    }
                    Err(Error::TcpConnection(e)) => {
                        error!("TCP error in main node loop, scheduling reconnect: {}", e);
                        self.schedule_reconnect().await;
                        continue
                    },
                    Err(e) => {
                        error!("error executing a set of jobs: {}", e);
                    }
                }
                //
            }
        })
    }

    async fn fetch_new_job(
        &mut self,
        initialized_job: JobIdentifier,
        after_building: bool,
    ) -> BuildTaskRunTask {
        loop {
            let (tx, rx) = oneshot::channel();
            let response = self
                .common
                .request_job_tx
                .send(JobRequest::from(NewJobRequest {
                    tx,
                    initialized_job,
                    after_building,
                    capabilities: self.common.capabilities.clone(),
                }))
                .await;

            match response {
                Ok(x) => x,
                Err(_) => {
                    error!("the job pool server has been dropped and cannot be accessed from {}. This is an irrecoverable error", &self.common.conn.addr);
                    panic!("the job pool server has been dropped and cannot be accessed from {}. This is an irrecoverable error", &self.common.conn.addr);
                }
            }

            match rx.await.unwrap() {
                JobResponse::SetupOrRun(t) => return t.flatten(),
                JobResponse::EmptyJobs => {
                    debug!(
                        "no more jobs to run on {} - sleeping and asking for more",
                        self.common.conn.addr
                    );
                    tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                    continue;
                }
            }
        }
    }

    async fn execute_job_set(
        &mut self,
        default_build: Option<BuildTaskInfo>,
    ) -> Result<BuildTaskInfo, Error> {
        // check to see if we were told what our build task would be already
        let build_info = if let Some(def) = default_build {
            def
        }
        // this is the first time running this function - we need to fetch
        // the building task ourselves
        else {
            let after_building = false;

            let task = self
                .fetch_new_job(JobIdentifier::None, after_building)
                .await;

            match task {
                BuildTaskRunTask::Build(b) => b,
                BuildTaskRunTask::Run(_) => {
                    return Err(Error::BuildJob(error::BuildJobError::SentExecutable))
                }
            }
        };

        // we now know what job it is that we are supposed to be building - lets build it
        let build = BuildingNode {
            common: &mut self.common,
            task_info: build_info,
        };

        let ready_executable = build.run_build().await?;
        let after_build = true;

        // constantly ask for more jobs related to the job that we have 
        // built on the node. If we receive a new job that is a build 
        // request then we return out of this function to repeat
        // the process from the top
        loop {
            let job_to_run = self
                .fetch_new_job(ready_executable.initialized_job, after_build)
                .await;

            // if we have been told to build a new set of tasks then we
            // return so that we can run this function from the top -
            // otherwise we keep going
            let job = match job_to_run {
                BuildTaskRunTask::Build(x) => return Ok(x),
                BuildTaskRunTask::Run(x) => x,
            };

            let running_node = RunningNode::new(&ready_executable, job, &mut self.common);
            // check that the task we were going to run was the task that we have currently
            // built
            if let Some(run) = running_node {
                run.execute_task().await?
            }
            // what we were asked to run did not make sense
            else {
                return Err(Error::RunningJob(error::RunningNodeError::MissingBuildStep));
            }
        }
        //
    }

    async fn schedule_reconnect(&mut self) {
        while let Err(e) = self._schedule_reconnect().await {
            error!(
                "could not reconnect to node at {}: {}",
                self.common.conn.addr, e
            );
        }
    }

    async fn _schedule_reconnect(&mut self) -> Result<(), Error> {
        tokio::time::sleep(Duration::from_secs(60)).await;
        self.common.conn.reconnect().await
    }
}

#[derive(derive_more::Constructor)]
pub(crate) struct Common {
    conn: transport::ServerConnection,
    request_job_tx: mpsc::Sender<JobRequest>,
    receive_cancellation: broadcast::Receiver<JobIdentifier>,
    capabilities: Arc<Requirements<NodeProvidedCaps>>,
    save_path: PathBuf,
}

struct BuildingNode<'a> {
    common: &'a mut Common,
    task_info: BuildTaskInfo,
}

impl<'a> BuildingNode<'a> {
    async fn run_build(self) -> Result<WaitingExecutableNode, Error> {
        let save_path: PathBuf = self
            .task_info
            .batch_save_path(&self.common.save_path)
            .await
            .map_err(|(error, path)| error::CreateDirError::new(error, path))
            .map_err(error::BuildJobError::from)?;

        let req = transport::RequestFromServer::from(self.task_info.task);
        self.common.conn.transport_data(&req).await?;

        let handle = handle_client_response::<Error>(&mut self.common.conn, &save_path);
        execute_with_cancellation(handle, &mut self.common.receive_cancellation, self.task_info.identifier)
            .await?;

        Ok(WaitingExecutableNode {
            initialized_job: self.task_info.identifier,
        })
    }
}

struct WaitingExecutableNode {
    initialized_job: JobIdentifier,
}

struct RunningNode<'a> {
    task_info: RunTaskInfo,
    common: &'a mut Common,
}

impl<'a> RunningNode<'a> {
    fn new(
        waiting_node: &WaitingExecutableNode,
        task_info: RunTaskInfo,
        common: &'a mut Common,
    ) -> Option<Self> {
        if waiting_node.initialized_job != task_info.identifier {
            error!("run task on {} scheduled from the job pool did not have the same identifier as us: {} (us) {} (given). This job will be lost",
                common.conn.addr, waiting_node.initialized_job, task_info.identifier);
            return None;
        }

        Some(Self { common, task_info })
    }

    async fn execute_task(self) -> Result<(), Error> {
        let save_path: PathBuf = self
            .task_info
            .batch_save_path(&self.common.save_path)
            .await
            .map_err(|(error, path)| error::CreateDirError::new(error, path))
            .map_err(error::RunningNodeError::from)?;

        let req = transport::RequestFromServer::from(self.task_info.task.clone());
        self.common
            .conn
            .transport_data(&req)
            .await?;


        let handle = handle_client_response::<Error>(&mut self.common.conn, &save_path);
        execute_with_cancellation(handle, &mut self.common.receive_cancellation, self.task_info.identifier)
            .await?;

        Ok(())
    }
}

// monitor the broadcast queue to see if a cancellation message is received
//
// this is broken out into a separate function since the tokio::select! requires
// two mutable borrows to &mut self
async fn check_cancellation(
    current_job: JobIdentifier,
    rx_cancel: &mut broadcast::Receiver<JobIdentifier>,
) {
    loop {
        if let Ok(identifier) = rx_cancel.recv().await {
            if current_job == identifier
            {
                return;
            }
        }
    }
}

/// execute a generic future returning a result while also checking for possible cancellations
/// from the job pool
async fn execute_with_cancellation<E>(
    fut: impl std::future::Future<Output = Result<(), E>>,
    cancel: &mut broadcast::Receiver<JobIdentifier>,
    current_ident: JobIdentifier,
) -> Result<(), E> {
    tokio::select!(
        response = fut => {
            response
        }
        _cancel_result = check_cancellation(current_ident, cancel) => {
            Ok(())
        }
    )
}

/// check what the client responded to the job request (not init) that we sent them
///
/// save any files that they sent us and return a simplified
/// version of the response they had
async fn handle_client_response<E>(
    conn: &mut transport::ServerConnection,
    save_path: &Path,
) -> Result<(), E>
where
    E: From<Error>,
{
    loop {
        let response = conn.receive_data().await?;
        match response {
            transport::ClientResponse::SendFile(send_file) => {
                receive_file(conn, &save_path, send_file)
                    .await?
            }
            transport::ClientResponse::RequestNewJob(_job_request) => {
                // we handle this at the call site of this function
                break;
            }
            transport::ClientResponse::StatusCheck(s) => {
                warn!("status check was received from the client on {}, we were expecting a file or job request: {}", &conn.addr, s);
                continue;
            }
            transport::ClientResponse::Error(e) => {
                warn!(
                    "client on {} experienced an error building the job: {:?}",
                    conn.addr, e
                );
                continue;
            }
        };
    }

    Ok(())
}

async fn receive_file(
    conn: &mut transport::ServerConnection,
    save_path: &Path,
    send_file: transport::SendFile,
) -> Result<(), Error> {
    // we need to store this file
    let save_location = save_path.join(send_file.file_path);
    info!("saving solver file to {:?}", save_location);
    if send_file.is_file {
        // TODO: fix these unwraps
        let mut file = tokio::fs::File::create(&save_location)
            .await
            .map_err(|error| error::WriteFile::from((error, save_location.clone())))
            .map_err(|e| error::ServerError::from(e))?;

        file.write_all(&send_file.bytes).await.unwrap();
    } else {
        // just create the directory
        ok_if_exists(tokio::fs::create_dir(&save_location).await)
            .map_err(|error| error::CreateDirError::from((error, save_location.clone())))
            .map_err(|e| error::ServerError::from(e))?;
    }

    // after we have received the file, let the client know this and send another
    // file
    conn.transport_data(&transport::RequestFromServer::FileReceived)
        .await?;

    Ok(())
}
enum NodeNextStep {
    RequestNextJob,
    ClientError,
}
