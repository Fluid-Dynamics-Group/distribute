use super::ok_if_exists;
use super::schedule::JobIdentifier;
use crate::config::requirements::{NodeProvidedCaps, Requirements};

use super::pool_data::{BuildTaskInfo, JobRequest, JobResponse, NewJobRequest, RunTaskInfo};
use crate::{error, error::Error, transport};

use super::pool_data;
use std::collections::BTreeSet;

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::AsyncWriteExt;

use tokio::sync::{broadcast, mpsc, oneshot};
use tokio::task::JoinHandle;

use crate::prelude::*;

//impl InitializedNode {
//    pub(super) fn spawn(mut self) -> JoinHandle<()> {
//        tokio::task::spawn(async move {
//            let mut last_set = None;
//            // now we just pull jobs from the server until we are done
//            loop {
//                // if we are at this point in the loop then we need to build
//                // a new job and execute it until there are no more remaining jobs
//                match self.execute_job_set(last_set.take()).await {
//                    Ok(new_set) => {
//                        last_set = Some(new_set);
//                        continue;
//                    }
//                    Err(LocalOrClientError::Local(Error::TcpConnection(e))) => {
//                        error!("TCP error in main node loop, scheduling reconnect: {}", e);
//                        self.schedule_reconnect().await;
//                        continue;
//                    }
//                    Err(e) => {
//                        error!("error executing a set of jobs: {}", e);
//                    }
//                }
//                //
//            }
//        })
//    }
//
//
//    async fn mark_job_finish(&mut self, initialized_job: JobIdentifier) {
//        let response = self
//            .common
//            .request_job_tx
//            .send(JobRequest::FinishJob(initialized_job))
//            .await;
//
//        match response {
//            Ok(x) => x,
//            Err(_) => {
//                error!("the job pool server has been dropped and cannot be accessed from {}. This is an irrecoverable error", &self.common.conn.addr);
//                panic!("the job pool server has been dropped and cannot be accessed from {}. This is an irrecoverable error", &self.common.conn.addr);
//            }
//        }
//    }
//
//    async fn execute_job_set(
//        &mut self,
//        default_build: Option<BuildTaskInfo>,
//    ) -> Result<BuildTaskInfo, LocalOrClientError> {
//        // check to see if we were told what our build task would be already
//        let build_info = if let Some(def) = default_build {
//            def
//        }
//        // this is the first time running this function - we need to fetch
//        // the building task ourselves
//        else {
//            let task = self.fetch_new_job(JobIdentifier::None).await;
//
//            match task {
//                BuildTaskRunTask::Build(b) => b,
//                BuildTaskRunTask::Run(_) => {
//                    return Err(Error::BuildJob(error::BuildJobError::SentExecutable))?
//                }
//            }
//        };
//
//        let job_ident = build_info.identifier;
//
//        // we now know what job it is that we are supposed to be building - lets build it
//        let build = BuildingNode {
//            common: &mut self.common,
//            task_info: build_info,
//        };
//
//        let ready_executable = build.run_build().await;
//
//        // if we had an error building this job then we mark it as being problematic so we
//        // dont end up trying to rebuild this file again
//        if matches!(ready_executable, Err(LocalOrClientError::ClientExecution)) {
//            self.errored_jobsets.insert(job_ident);
//        }
//
//        let ready_executable = ready_executable?;
//
//        // constantly ask for more jobs related to the job that we have
//        // built on the node. If we receive a new job that is a build
//        // request then we return out of this function to repeat
//        // the process from the top
//        loop {
//            let job_to_run = self.fetch_new_job(ready_executable.initialized_job).await;
//
//            // if we have been told to build a new set of tasks then we
//            // return so that we can run this function from the top -
//            // otherwise we keep going
//            let job = match job_to_run {
//                BuildTaskRunTask::Build(x) => return Ok(x),
//                BuildTaskRunTask::Run(x) => x,
//            };
//
//            let running_node = RunningNode::new(&ready_executable, job, &mut self.common);
//
//            // check that the task we were going to run was the task that we have currently
//            // built
//            if let Some(mut run) = running_node {
//                // execute the job
//                if let Err(e) = run.execute_task().await {
//                    match e {
//                        LocalOrClientError::KeepaliveFailure => {
//                            // we dont want to make this job not executed EVER, we should add it
//                            // back to the job queue so that someone else can take care of it while
//                            // this node is down
//                            info!("adding job back to queue after timeout failure:");
//                            return_job_to_queue(run.common, run.task_info).await;
//                        }
//                        _ => {
//                            error!("could not execute the task on the client due to an error - marking this job as finished - {}", e);
//                            self.mark_job_finish(ready_executable.initialized_job).await;
//                        }
//                    }
//
//                    return Err(e);
//                } else {
//                    self.mark_job_finish(ready_executable.initialized_job).await;
//                }
//            }
//            // what we were asked to run did not make sense
//            else {
//                error!("Could not initialize a node to run on - the job passed from the scheduler does not make sense - really bad shit might happen now");
//                return Err(Error::RunningJob(error::RunningNodeError::MissingBuildStep))?;
//            }
//        }
//        //
//    }
//
//    async fn schedule_reconnect(&mut self) {
//        while let Err(e) = self._schedule_reconnect().await {
//            error!(
//                "could not reconnect to node at {}: {}",
//                self.common.conn.addr, e
//            );
//        }
//    }
//
//    async fn _schedule_reconnect(&mut self) -> Result<(), Error> {
//        tokio::time::sleep(Duration::from_secs(60)).await;
//        self.common.conn.reconnect().await
//    }
//}

pub(crate) async fn fetch_new_job(
    scheduler_tx: &mut mpsc::Sender<JobRequest>,
    initialized_job: JobIdentifier,
    node_name: &str,
    info_addr: &SocketAddr,
    keepalive_addr: &SocketAddr,
    capabilities: Arc<Requirements<NodeProvidedCaps>>,
    errored_jobsets: BTreeSet<JobIdentifier>,
) -> pool_data::FetchedJob {
    loop {
        if let Err(e) = check_keepalive(keepalive_addr, node_name).await {
            info!("{} could not access the client before fetching a new job from the server (err: {})- scheduling a reconnect", node_name, e);
            return pool_data::FetchedJob::MissedKeepalive;
        }

        let (tx, rx) = oneshot::channel();
        let response = scheduler_tx
            .send(JobRequest::from(NewJobRequest {
                tx,
                initialized_job,
                capabilities: capabilities.clone(),
                build_failures: errored_jobsets.clone(),
            }))
            .await;

        match response {
            Ok(x) => x,
            Err(_) => {
                error!("the job pool server has been dropped and cannot be accessed from {}. This is an irrecoverable error", &node_name);
                panic!("the job pool server has been dropped and cannot be accessed from {}. This is an irrecoverable error", &node_name);
            }
        }

        match rx.await.unwrap() {
            JobResponse::SetupOrRun(t) => return t.flatten(),
            JobResponse::EmptyJobs => {
                debug!(
                    "no more jobs to run on {} - sleeping and asking for more",
                    node_name
                );
                tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                continue;
            }
        }
    }
}

// /// return a job that failed to execute (for valid reasons) back to the queue
// /// this is a broken-out function since mutable borrowing rules
// async fn return_job_to_queue(common: &mut Common, info: RunTaskInfo) {
//     let response = common.request_job_tx.send(JobRequest::DeadNode(info)).await;
//
//     match response {
//         Ok(_) => (),
//         Err(_) => {
//             error!("the job pool server has been dropped and cannot be accessed from {}. This is an irrecoverable error", common.conn.addr);
//             panic!("the job pool server has been dropped and cannot be accessed from {}. This is an irrecoverable error", common.conn.addr);
//         }
//     }
// }

//struct BuildingNode<'a> {
//    common: &'a mut Common,
//    task_info: BuildTaskInfo,
//}

//impl<'a> BuildingNode<'a> {
//    async fn run_build(self) -> Result<WaitingExecutableNode, LocalOrClientError> {
//        let save_path: PathBuf = self
//            .task_info
//            .batch_save_path(&self.common.save_path)
//            .await
//            .map_err(|(error, path)| error::CreateDirError::new(error, path))
//            .map_err(error::BuildJobError::from)
//            .map_err(Error::from)?;
//
//        let req = transport::RequestFromServer::from(self.task_info.task);
//        self.common.conn.transport_data(&req).await?;
//
//        let keepalive_check = complete_on_ping_failure(self.common.conn.addr.clone());
//
//        let handle =
//            handle_client_response::<LocalOrClientError>(&mut self.common.conn, &save_path);
//
//        let _execution = execute_with_cancellation(
//            handle,
//            &mut self.common.receive_cancellation,
//            self.task_info.identifier,
//            keepalive_check,
//        )
//        .await?;
//
//        Ok(WaitingExecutableNode {
//            initialized_job: self.task_info.identifier,
//        })
//    }
//}

//struct WaitingExecutableNode {
//    initialized_job: JobIdentifier,
//}
//
//struct RunningNode<'a> {
//    task_info: RunTaskInfo,
//    common: &'a mut Common,
//}
//
//impl<'a> RunningNode<'a> {
//    fn new(
//        waiting_node: &WaitingExecutableNode,
//        task_info: RunTaskInfo,
//        common: &'a mut Common,
//    ) -> Option<Self> {
//        if waiting_node.initialized_job != task_info.identifier {
//            error!("run task on {} scheduled from the job pool did not have the same identifier as us: {} (us) {} (given). This job will be lost",
//                common.conn.addr, waiting_node.initialized_job, task_info.identifier);
//            return None;
//        }
//
//        Some(Self { common, task_info })
//    }
//
//    async fn execute_task(&mut self) -> Result<(), LocalOrClientError> {
//        let save_path: PathBuf = self
//            .task_info
//            .batch_save_path(&self.common.save_path)
//            .await
//            .map_err(|(error, path)| error::CreateDirError::new(error, path))
//            .map_err(error::RunningNodeError::from)
//            .map_err(Error::from)?;
//
//        let req = transport::RequestFromServer::from(self.task_info.task.clone());
//        self.common.conn.transport_data(&req).await?;
//
//        let keepalive_check = complete_on_ping_failure(self.common.conn.addr.clone());
//
//        let handle =
//            handle_client_response::<LocalOrClientError>(&mut self.common.conn, &save_path);
//
//        execute_with_cancellation(
//            handle,
//            &mut self.common.receive_cancellation,
//            self.task_info.identifier,
//            keepalive_check,
//        )
//        .await?;
//
//        Ok(())
//    }
//}

// // monitor the broadcast queue to see if a cancellation message is received
// //
// // this is broken out into a separate function since the tokio::select! requires
// // two mutable borrows to &mut self
// async fn check_cancellation(
//     current_job: JobIdentifier,
//     rx_cancel: &mut broadcast::Receiver<JobIdentifier>,
// ) {
//     loop {
//         if let Ok(identifier) = rx_cancel.recv().await {
//             if current_job == identifier {
//                 return;
//             }
//         }
//     }
// }

// /// execute a generic future returning a result while also checking for possible cancellations
// /// from the job pool
// async fn execute_with_cancellation<E>(
//     fut: impl std::future::Future<Output = Result<(), E>>,
//     cancel: &mut broadcast::Receiver<JobIdentifier>,
//     current_ident: JobIdentifier,
//     check_keepalive: impl std::future::Future<Output = ()>,
// ) -> Result<(), E>
// where
//     E: From<KeepaliveError>,
// {
//     tokio::select!(
//         response = fut => {
//             response
//         }
//         _cancel_result = check_cancellation(current_ident, cancel) => {
//             Ok(())
//         }
//         _keepalive_result = check_keepalive => {
//             error!("execute_with_cancellation ending due to keepalive failure");
//             Ok(())
//         }
//     )
// }

/// constantly polls a connection to ensure that
async fn complete_on_ping_failure(address: std::net::SocketAddr, name: &str) -> () {
    loop {
        if let Err(e) = check_keepalive(&address, name).await {
            error!(
                "error checking the keepalive for node at {}: {}",
                address, e
            );
            return ();
        }

        #[cfg(not(test))]
        tokio::time::sleep(Duration::from_secs(6 * 60)).await;

        #[cfg(test)]
        tokio::time::sleep(Duration::from_secs(10)).await;
    }
}

/// ping an address and
async fn check_keepalive(address: &std::net::SocketAddr, name: &str) -> Result<(), Error> {
    // TODO: this connection might be able to stall, im not sure
    let mut conn = transport::Connection::new(*address).await?;
    conn.transport_data(&transport::ServerQuery::KeepaliveCheck)
        .await?;

    match tokio::time::timeout(Duration::from_secs(10), conn.receive_data()).await {
        Ok(inner) => {
            inner?;
            ()
        }
        Err(_elapsed) => return Err(error::TimeoutError::new(*address, name.to_string()).into()),
    }

    Ok(())
}
