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
use crate::protocol;
use crate::protocol::UninitServer;

pub(crate) async fn run_node(
    common: protocol::Common,
    mut scheduler_tx: mpsc::Sender<server::JobRequest>,
) {
    info!(
        "initializing connection to {} / {}",
        common.node_name, common.main_transport_addr
    );

    let conn = make_connection(common.main_transport_addr, &common.node_name).await;

    let ip = common.main_transport_addr;
    let name = common.node_name.clone();

    let mut uninit = protocol::Machine::<_, protocol::uninit::ServerUninitState>::new(conn, common);

    loop {
        match run_all_jobs(uninit, &mut scheduler_tx).await {
            Ok(_) => unreachable!(),
            Err((_uninit, error)) => {
                uninit = _uninit;

                if error.is_tcp_error() {
                    error!(
                        "tcp error encountered when executing jobs on {} / {}: {}",
                        ip, name, error
                    );

                    let conn = make_connection(ip, &name).await;

                    uninit.update_tcp_connection(conn);
                } else {
                    error!(
                        "non tcp error encountered when executing jobs on {} / {}: {}",
                        ip, name, error
                    );
                }
            }
        };
    }
}

/// take an uninitialized client and push it through the state machine loop
async fn run_all_jobs(
    uninit: protocol::UninitServer,
    scheduler_tx: &mut mpsc::Sender<server::JobRequest>,
) -> Result<(), (protocol::UninitServer, protocol::ServerError)> {
    let prepare_build = match uninit.connect_to_node().await {
        Ok(prep) => prep,
        Err((uninit, err)) => return Err((uninit, err.into())),
    };

    let mut built = prepare_build_to_built(prepare_build, scheduler_tx).await?;

    #[allow(unreachable_code)]
    loop {
        // now that we have compiled, we should
        let execute = match built.send_job_execution_instructions(scheduler_tx).await {
            Ok(protocol::Either::Right(executing)) => executing,
            Ok(protocol::Either::Left(prepare_build)) => {
                built = prepare_build_to_built(prepare_build, scheduler_tx).await?;
                continue;
            }
            Err((built, err)) => return Err((built.to_uninit(), err.into())),
        };

        // fully execute the job and return back to the built state
        match execute_and_send_files(execute, scheduler_tx).await? {
            // we were probably cancelled while executing the job
            protocol::Either::Left(prepare_build) => {
                built = prepare_build_to_built(prepare_build, scheduler_tx).await?;
            }
            // we successfully finished the job, we are good to
            // move back to the built state
            protocol::Either::Right(_built) => {
                built = _built;
            }
        }
    }
}

/// execute the job on the client and receive all the files they send to us
async fn execute_and_send_files(
    execute: protocol::ExecuteServer,
    scheduler_tx: &mut mpsc::Sender<server::JobRequest>,
) -> Result<
    protocol::Either<protocol::PrepareBuildServer, protocol::BuiltServer>,
    (UninitServer, protocol::ServerError),
> {
    let send_files = match execute.wait_job_execution().await {
        Ok(protocol::Either::Right(send)) => send,
        Ok(protocol::Either::Left(prepare_build)) => {
            return Ok(protocol::Either::Left(prepare_build))
        }
        Err((execute, err)) => {
            error!("error executing the job: {}", err);
            return Err((execute.to_uninit(), err.into()));
        }
    };

    let built = match send_files.receive_files(scheduler_tx).await {
        Ok(built) => built,
        Err((send_files, err)) => {
            error!("error sending result files from the job: {}", err);
            return Err((send_files.to_uninit(), err.into()));
        }
    };

    Ok(protocol::Either::Right(built))
}

/// take a [`protocol::PrepareBuildServer`] and map it to a [`protocol::BuiltServer`]
async fn prepare_build_to_built(
    prepare_build: protocol::PrepareBuildServer,
    scheduler_tx: &mut mpsc::Sender<server::JobRequest>,
) -> Result<protocol::BuiltServer, (UninitServer, protocol::ServerError)> {
    let mut building_state_or_prepare =
        inner_prepare_build_to_compile_result(prepare_build, scheduler_tx).await?;

    let built: protocol::BuiltServer;

    loop {
        // if we are here, we dont know if the return value from the compilation
        // in .build_job() was successfull or not. We need to check:
        match building_state_or_prepare {
            protocol::Either::Left(_built) => {
                built = _built;
                break;
            }
            protocol::Either::Right(prepare_build) => {
                building_state_or_prepare =
                    inner_prepare_build_to_compile_result(prepare_build, scheduler_tx).await?;
                continue;
            }
        };
    }

    Ok(built)
}

/// iterate over a [`protocol::PrepareBuildServer`]
/// and map it to _either_ a [`protocol::BuiltServer`]
/// or a  [`protocol::`](`protocol::PrepareBuildServer`)
async fn inner_prepare_build_to_compile_result(
    prepare_build: protocol::PrepareBuildServer,
    scheduler_tx: &mut mpsc::Sender<server::JobRequest>,
) -> Result<
    protocol::Either<protocol::BuiltServer, protocol::PrepareBuildServer>,
    (UninitServer, protocol::ServerError),
> {
    let building_state = match prepare_build.send_job(scheduler_tx).await {
        Ok(building) => building,
        Err((prepare_build, err)) => return Err((prepare_build.to_uninit(), err.into())),
    };

    let building_state_or_prepare = match building_state.prepare_for_execution().await {
        Ok(built) => built,
        Err((prepare_build, err)) => return Err((prepare_build.to_uninit(), err.into())),
    };

    Ok(building_state_or_prepare)
}

async fn make_connection(addr: SocketAddr, name: &str) -> tokio::net::TcpStream {
    loop {
        info!("making server TCP connection to {}", addr);
        match tokio::net::TcpStream::connect(addr).await {
            Ok(conn) => return conn,
            Err(e) => {
                error!("failed to connect to node {} at {} to create uninitialized connection - sleeping for 5 minutes", name, addr);

                tokio::time::sleep(Duration::from_secs(60 * 5)).await;

                continue;
            }
        };
    }
}

pub(crate) async fn fetch_new_job(
    scheduler_tx: &mut mpsc::Sender<JobRequest>,
    initialized_job: JobIdentifier,
    node_name: &str,
    _transport_addr: &SocketAddr,
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
    trace!("making keepalive check to {}", address);
    // TODO: this connection might be able to stall, im not sure
    let mut conn = transport::Connection::new(*address).await?;
    conn.transport_data(&transport::ServerQuery::KeepaliveCheck)
        .await?;

    match tokio::time::timeout(Duration::from_secs(10), conn.receive_data()).await {
        Ok(inner) => match inner {
            Ok(x) => {
                trace!("keepalive was successful -> {:?}", x);
                Ok(())
            }
            Err(e) => {
                trace!("keepalive failed: {:?}", e);
                Err(e.into())
            }
        },
        Err(_elapsed) => Err(error::TimeoutError::new(*address, name.to_string()).into()),
    }
}
