use super::pool_data::{CancelResult, JobRequest, JobResponse};
use super::schedule::{JobSetIdentifier, Schedule};
use crate::prelude::*;

use tokio::task::JoinHandle;

use derive_more::Constructor;

#[derive(Constructor)]
pub(super) struct JobPool<T> {
    remaining_jobs: T,
    receive_requests: mpsc::Receiver<JobRequest>,
    broadcast_cancel: broadcast::Sender<JobSetIdentifier>,
    total_nodes: usize,
}

impl<T> JobPool<T>
where
    T: Schedule + Send + 'static,
{
    pub(super) fn spawn(mut self) -> JoinHandle<()> {
        tokio::task::spawn(async move {
            while let Some(new_req) = self.receive_requests.recv().await {
                match new_req {
                    JobRequest::FinishJob(finish) => {
                        info!(
                            "marking a finished job for {} ({})",
                            finish.job_name, finish.ident
                        );
                        self.remaining_jobs
                            .finish_job(finish.ident, &finish.job_name);
                    }
                    // we want a new job from the scheduler
                    JobRequest::NewJob(new_req) => {
                        debug!("a node has asked for a new job");
                        // if we are requesting a new job -not- right after building a job

                        let new_task: JobResponse = self.remaining_jobs.fetch_new_task(
                            new_req.initialized_job,
                            new_req.capabilities,
                            &new_req.build_failures,
                            new_req.node_meta,
                        );
                        new_req.tx.send(new_task).ok().unwrap();
                    }
                    // a job failed to execute on the node
                    JobRequest::DeadNode(pending_job) => {
                        debug!("a node has died for now, the job is returning to the scheduler");
                        self.remaining_jobs
                            .add_job_back(pending_job.task, pending_job.identifier);

                        continue;
                    }
                    // the server got a request to add a new job set
                    JobRequest::AddJobSet(set) => {
                        info!("added new job set `{}` to scheduler", set.batch_name());
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
                            if self.broadcast_cancel.send(found_identifier).is_ok() {
                                debug!(
                                    "successfully sent cancellation message for batch name {}",
                                    &cancel_query.batch_name
                                );

                                self.remaining_jobs.cancel_batch(found_identifier);

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
                    JobRequest::MarkBuildFailure(failure) => {
                        self.remaining_jobs
                            .mark_build_failure(failure.ident, self.total_nodes);
                    }
                };
                //
            }
        })
    }
}
