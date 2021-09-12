use super::ok_if_exists;
use super::schedule::{JobIdentifier, NodeProvidedCaps, Requirements, Schedule};
use crate::{cli, config, error, error::Error, status, transport};

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

use derive_more::From;

#[derive(derive_more::Constructor)]
pub(super) struct JobPool<T> {
    remaining_jobs: T,
    receive_requests: mpsc::Receiver<JobRequest>,
}

impl<T> JobPool<T>
where
    T: Schedule + Send + 'static,
{
    pub(super) fn spawn(mut self) -> JoinHandle<()> {
        tokio::task::spawn(async move {
            while let Some(new_req) = self.receive_requests.recv().await {
                let new_req = match new_req {
                    JobRequest::NewJob(x) => x,
                    JobRequest::DeadNode(pending_job) => {
                        let job = match pending_job.task {
                            JobOrInit::Job(job) => job,
                            JobOrInit::JobInit(init) => continue,
                        };

                        self.remaining_jobs.add_job_back(job, pending_job.ident);
                        continue;
                    }
                };

                let new_task: JobResponse = self
                    .remaining_jobs
                    .fetch_new_task(new_req.initialized_job, new_req.capabilities);
                new_req.tx.send(new_task).ok().unwrap();
            }
        })
    }
}

pub(crate) enum JobResponse {
    SetupOrRun {
        task: JobOrInit,
        identifier: JobIdentifier,
    },
    EmptyJobs,
}

#[derive(derive_more::From)]
pub(super) enum JobRequest {
    NewJob(NewJobRequest),
    DeadNode(PendingJob),
}

pub(super) struct NewJobRequest {
    tx: oneshot::Sender<JobResponse>,
    initialized_job: JobIdentifier,
    capabilities: Arc<Requirements<NodeProvidedCaps>>,
}

#[derive(Clone)]
pub(super) struct PendingJob {
    task: JobOrInit,
    ident: JobIdentifier,
}

#[cfg_attr(test, derive(derive_more::Unwrap))]
#[derive(From, Clone)]
pub(crate) enum JobOrInit {
    Job(transport::Job),
    JobInit(transport::JobInit),
}

#[derive(derive_more::Constructor)]
pub(super) struct NodeConnection {
    conn: transport::ServerConnection,
    request_job_tx: mpsc::Sender<JobRequest>,
    capabilities: Arc<Requirements<NodeProvidedCaps>>,
    save_path: PathBuf,
    current_job: Option<PendingJob>,
}

impl NodeConnection {
    pub(super) fn spawn(mut self) -> JoinHandle<()> {
        tokio::task::spawn(async move {
            // now we just pull jobs from the server until we are done
            while let Ok(response) = self.fetch_new_job().await {
                info!("launching new job for node {}", self.addr());

                // load the response into `self.current_job` or
                // pause for a bit while we wait for more jobs to be added to the
                // server
                let new_job = match response {
                    JobResponse::SetupOrRun { task, identifier } => {
                        self.current_job = Some(PendingJob {
                            task,
                            ident: identifier,
                        });
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

                if let Err(e) = send_response {
                    error!(
                        "error sending the request to the server on node at {}: {}",
                        self.addr(),
                        e
                    );

                    self.request_job_tx
                        .send(JobRequest::DeadNode(self.current_job.clone().unwrap()))
                        .await
                        .ok()
                        .unwrap();

                    continue;
                }
                // we ere able to send the data to the client without issue
                else {
                    // check if we could read from the TCP socket without issue:
                    match self.handle_client_response().await {
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
                    .map(|x| x.ident)
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

    /// check what the client responded to the job request (not init) that we sent them
    ///
    /// save any files that they sent us and return a simplified
    /// version of the response they had
    async fn handle_client_response(&mut self) -> Result<NodeNextStep, Error> {
        loop {
            let response = self.conn.receive_data().await?;

            match response {
                transport::ClientResponse::StatusCheck(_) => todo!(),
                transport::ClientResponse::RequestNewJob(_) => {
                    return Ok(NodeNextStep::RequestNextJob)
                }
                transport::ClientResponse::SendFile(send_file) => {
                    // we need to store this file
                    let save_location = self.save_path.join(send_file.file_path);
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
                }
                _ => unimplemented!(),
            }
        }
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

enum NodeNextStep {
    RequestNextJob,
    ClientError,
}
