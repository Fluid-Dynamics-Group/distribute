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

use derive_more::{From, Constructor};

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
                                self.remaining_jobs.add_job_back(job, pending_job.identifier);
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

pub(crate) enum CancelResult {
    BatchNameMissing,
    NoBroadcastNodes,
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
    identifier: JobIdentifier,
    task: JobOrInit,
}

impl TaskInfo {
    async fn batch_save_path(&self, base_path: &Path) -> Result<PathBuf, std::io::Error> {
        let path = match &self.task {
            JobOrInit::Job(jobopt) => base_path.join(&self.namespace).join(&self.batch_name).join(jobopt.name()),
            JobOrInit::JobInit(init) => base_path.join(&self.namespace).join(&self.batch_name)
        };

        debug!("creating path {}", path.display());
        ok_if_exists(tokio::fs::create_dir_all(&path).await)?;

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
pub(super) struct NodeConnection {
    conn: transport::ServerConnection,
    request_job_tx: mpsc::Sender<JobRequest>,
    receive_cancellation: broadcast::Receiver<JobIdentifier>,
    capabilities: Arc<Requirements<NodeProvidedCaps>>,
    save_path: PathBuf,
    current_job: Option<TaskInfo>,
}

impl NodeConnection {
    pub(super) fn spawn(mut self) -> JoinHandle<()> {
        tokio::task::spawn(async move {
            // now we just pull jobs from the server until we are done

            // TODO: if there is an error in the handling of the response the loop will recycle 
            // and the client will get a new connection instead of a response like FileReceived
            // which may destroy everything
            while let Ok(response) = self.fetch_new_job().await {
                info!("request for job on node {}", self.addr());

                // load the response into `self.current_job` or
                // pause for a bit while we wait for more jobs to be added to the
                // server
                let new_job = match response {
                    JobResponse::SetupOrRun(task_info)=> {
                        self.current_job = Some(task_info);
                        &self.current_job.as_ref().unwrap().task
                    }
                    JobResponse::EmptyJobs => {
                        tokio::time::sleep(Duration::from_secs(60)).await;
                        continue;
                    }
                };

                let server_request = match new_job {
                    JobOrInit::Job(job) => transport::RequestFromServer::from(job.clone()),
                    JobOrInit::JobInit(init) => transport::RequestFromServer::from(init.clone()),
                };

                let send_response = self.conn.transport_data(&server_request).await;

                // we were not able to send the job to the node, send it back
                // to the scheduler so that someone else can take it instead
                if let Err(e) = send_response {
                    error!(
                        "error sending the request to the server on node at {}: {}",
                        self.addr(),
                        e
                    );

                    self.request_job_tx
                        .send(JobRequest::DeadNode(self.current_job.take().unwrap()))
                        .await
                        .ok()
                        .unwrap();

                    // since it was a TCP error we reconnect to the node
                    // before doing anything else
                    self.schedule_reconnect().await;

                    continue;
                }
                // we were able to send the data to the client without issue
                else {
                    let addr = self.addr().clone();
                    let current_job = self.current_job.as_ref();
                    let broadcast = &mut self.receive_cancellation;

                    let save_path = &self.save_path;
                    let conn = &mut self.conn;

                    // todo: might repeat work here
                    let batch_save_path = if let Ok(x) = self
                        .current_job
                        .as_ref()
                        .unwrap()
                        .batch_save_path(&save_path)
                        .await
                    {
                        x
                    } else {
                        warn!(
                            "could not create storage path for currnet batch on {}",
                            addr
                        );
                        save_path.to_owned()
                    };

                    let job_result = tokio::select!(
                        // check if we could read from the TCP socket without issue:
                        response = handle_client_response(conn, &batch_save_path) => {
                            match response {
                                Ok(response) => {
                                    // TODO: add some logic to schedule other jobs on this node
                                    // if this job was a initialization job
                                    // since that means that the packages were not correctly built on the
                                    // machine
                                }
                                Err(e) => {
                                    error!(
                                        "error receiving job response from node {}: {}",
                                        self.addr(),
                                        e
                                    );
                                    self.schedule_reconnect().await;
                                }
                            };

                            true
                        }
                        // check if this message was cancelled. the function will return when any
                        // message in the broadcast queue matches that of the current value of
                        // self.current_job
                        _ = check_cancellation(current_job, broadcast) => {
                            false
                        }
                    );

                    // check if the job was cancelled
                    if job_result == false {
                        // the job was cancelled, spin off a separate connection to the node so
                        // that we can message them
                        match transport::ServerConnection::new(self.conn.addr).await {
                            Ok(mut tmp_conn) => {
                                if let Err(e) = tmp_conn
                                    .transport_data(&transport::RequestFromServer::KillJob)
                                    .await
                                {
                                    error!("could not kill the job on the node: {}", e);
                                }

                                // we dont expect the child to reply with a message in this
                                // instance, we can kill the job and continue
                                //
                                // this is because the child may have already sent us a message
                                // for the finished job and there would be a race condition created
                            }
                            Err(e) => {
                                error!("failed to form temp connection to {} to notify of task cancellation. Setting current job to None
                                       and scheduling a reconnect on the main thread. Error: {}", self.addr(), e);
                                self.current_job = None;
                                self.schedule_reconnect().await;
                            }
                        }
                    }
                }
            }
        })
    }

    async fn fetch_new_job(&mut self) -> Result<JobResponse, mpsc::error::SendError<JobRequest>> {
        let (tx, rx) = oneshot::channel();
        self.request_job_tx
            .send(JobRequest::from(NewJobRequest {
                tx,
                initialized_job: self
                    .current_job
                    .clone()
                    .map(|x| x.identifier)
                    .or(Some(JobIdentifier::none()))
                    .unwrap(),
                capabilities: self.capabilities.clone(),
            }))
            .await?;

        Ok(rx.await.unwrap())
    }

    fn addr(&self) -> &std::net::SocketAddr {
        &self.conn.addr
    }

    async fn schedule_reconnect(&mut self) {
        while let Err(e) = self._schedule_reconnect().await {
            error!("could not reconnect to node at {}: {}", self.addr(), e);
        }
    }

    async fn _schedule_reconnect(&mut self) -> Result<(), Error> {
        tokio::time::sleep(Duration::from_secs(60)).await;
        self.conn.reconnect().await
    }
}

// monitor the broadcast queue to see if a cancellation message is received
//
// this is broken out into a separate function since the tokio::select! requires
// two mutable borrows to &mut self
async fn check_cancellation(
    current_job: Option<&TaskInfo>,
    rx_cancel: &mut broadcast::Receiver<JobIdentifier>,
) {
    loop {
        if let Ok(identifier) = rx_cancel.recv().await {
            if current_job
                .map(|job| job.identifier == identifier)
                .unwrap_or(false)
            {
                return;
            }
        }
    }
}

/// check what the client responded to the job request (not init) that we sent them
///
/// save any files that they sent us and return a simplified
/// version of the response they had
async fn handle_client_response(
    conn: &mut transport::ServerConnection,
    save_path: &Path,
) -> Result<NodeNextStep, Error> {
    loop {
        let response = conn.receive_data().await?;

        match response {
            transport::ClientResponse::StatusCheck(_) => todo!(),
            transport::ClientResponse::RequestNewJob(_) => return Ok(NodeNextStep::RequestNextJob),
            transport::ClientResponse::SendFile(send_file) => {
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
                        .map_err(|error| {
                            error::CreateDirError::from((error, save_location.clone()))
                        })
                        .map_err(|e| error::ServerError::from(e))?;
                }

                // after we have received the file, let the client know this and send another
                // file
                conn.transport_data(&transport::RequestFromServer::FileReceived)
                    .await?;
            }
            // this is possible if we errored handlign teh response from a thread and now we 
            // have started a new tcp connection, the responses dont have to be the ones 
            // above
            _ => unimplemented!(),
        }
    }
}
enum NodeNextStep {
    RequestNextJob,
    ClientError,
}
