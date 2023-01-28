use super::schedule::JobSetIdentifier;
use crate::config::requirements::{NodeProvidedCaps, Requirements};

use super::pool_data::{JobRequest, JobResponse, NewJobRequest};
use crate::{error, error::Error, transport};

use super::pool_data;
use std::collections::BTreeSet;

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{broadcast, mpsc, oneshot};

use crate::prelude::*;
use crate::protocol;
use crate::protocol::UninitServer;

pub(crate) async fn run_node(
    common: protocol::Common,
    mut scheduler_tx: mpsc::Sender<server::JobRequest>,
) {
    info!("initializing connection to {}", common.node_meta);

    let conn = make_connection(&common.node_meta).await;
    let node_meta = common.node_meta.clone();

    let mut uninit = protocol::Machine::<_, protocol::uninit::ServerUninitState>::new(conn, common);

    loop {
        match run_all_jobs(uninit, &mut scheduler_tx).await {
            Ok(_) => unreachable!(),
            Err((_uninit, error)) => {
                uninit = _uninit;

                if error.is_tcp_error() {
                    error!("tcp error encountered when executing jobs on {node_meta} : {error}",);

                    let conn = make_connection(&node_meta).await;

                    uninit.update_tcp_connection(conn);
                } else {
                    error!("non tcp error encountered when executing jobs on {node_meta}: {error}",);
                }
            }
        };

        info!("sleeping 10 seconds before restarting from init stage");
        tokio::time::sleep(Duration::from_secs(10)).await
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
        let send_execute_files = match built.send_job_execution_instructions(scheduler_tx).await {
            Ok(protocol::Either::Right(executing)) => executing,
            Ok(protocol::Either::Left(prepare_build)) => {
                built = prepare_build_to_built(prepare_build, scheduler_tx).await?;
                continue;
            }
            Err((built, err)) => return Err((built.into_uninit(), err.into())),
        };

        let execute = match send_execute_files.send_files().await {
            Ok(execute) => execute,
            Err((prepare_build, err)) => return Err((prepare_build.into_uninit(), err.into())),
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
    let send_files = match execute.wait_job_execution(scheduler_tx).await {
        Ok(protocol::Either::Right(send)) => send,
        Ok(protocol::Either::Left(prepare_build)) => {
            return Ok(protocol::Either::Left(prepare_build))
        }
        Err((execute, err)) => {
            error!(
                "{} error executing the job: {}, returning the job back to scheduler",
                execute.node_name(),
                err
            );

            let (uninit, run_task_info) = execute.into_uninit();

            let return_msg = server::JobRequest::DeadNode(run_task_info);
            scheduler_tx
                .send(return_msg)
                .await
                .ok()
                .expect("scheduler is offine - irrecoverable error");

            return Err((uninit, err.into()));
        }
    };

    let built = match send_files.receive_files(scheduler_tx).await {
        Ok(built) => built,
        Err((send_files, err)) => {
            error!(
                "{} error sending result files from the job: {}",
                send_files.node_name(),
                err
            );

            //
            // add the job back into the scheduler since it did not finish properly
            //
            let node_name = send_files.node_name().to_string();
            let job_name = send_files.job_name().to_string();

            let (uninit, run_task_info) = send_files.into_uninit();

            info!("{} adding job `{}` from `{}` back to scheduler since it failed to send its files previously", 
                node_name,
                job_name,
                run_task_info.batch_name,
            );

            let return_msg = server::JobRequest::DeadNode(run_task_info);
            scheduler_tx
                .send(return_msg)
                .await
                .ok()
                .expect("scheduler is offine - irrecoverable error");

            //
            // return out with the uninitialized state
            //

            return Err((uninit, err.into()));
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
    let send_compiling_state = match prepare_build.send_job(scheduler_tx).await {
        Ok(building) => building,
        Err((prepare_build, err)) => return Err((prepare_build.into_uninit(), err.into())),
    };

    let building_state = match send_compiling_state.send_files().await {
        Ok(building) => building,
        Err((prepare_build, err)) => return Err((prepare_build.into_uninit(), err.into())),
    };

    let built_state_or_prepare = match building_state.prepare_for_execution().await {
        Ok(built) => built,
        Err((prepare_build, err)) => return Err((prepare_build.into_uninit(), err.into())),
    };

    Ok(built_state_or_prepare)
}

//async fn make_connection(addr: SocketAddr, name: &str) -> tokio::net::TcpStream {
async fn make_connection(node_meta: &pool_data::NodeMetadata) -> tokio::net::TcpStream {
    loop {
        info!("making server TCP connection to {}", node_meta);
        match tokio::net::TcpStream::connect(node_meta.node_address).await {
            Ok(conn) => return conn,
            Err(e) => {
                error!("failed to connect to node {} to create uninitialized connection - sleeping for 5 minutes. err: `{e}`", node_meta);

                tokio::time::sleep(Duration::from_secs(60 * 5)).await;

                continue;
            }
        };
    }
}

pub(crate) async fn fetch_new_job(
    scheduler_tx: &mut mpsc::Sender<JobRequest>,
    initialized_job: JobSetIdentifier,
    node_meta: &pool_data::NodeMetadata,
    keepalive_addr: &SocketAddr,
    capabilities: Arc<Requirements<NodeProvidedCaps>>,
    errored_jobsets: BTreeSet<JobSetIdentifier>,
) -> pool_data::FetchedJob {
    loop {
        if let Err(e) = check_keepalive(keepalive_addr, node_meta).await {
            info!("{node_meta} could not access the client before fetching a new job from the server (err: {e})- scheduling a reconnect");
            return pool_data::FetchedJob::MissedKeepalive;
        }

        let (tx, rx) = oneshot::channel();
        let response = scheduler_tx
            .send(JobRequest::from(NewJobRequest {
                tx,
                initialized_job,
                capabilities: capabilities.clone(),
                build_failures: errored_jobsets.clone(),
                node_meta: node_meta.clone(),
            }))
            .await;

        match response {
            Ok(x) => x,
            Err(_) => {
                error!("the job pool server has been dropped and cannot be accessed from {node_meta}. This is an irrecoverable error");
                panic!("the job pool server has been dropped and cannot be accessed from {node_meta}. This is an irrecoverable error");
            }
        }

        match rx.await.unwrap() {
            JobResponse::SetupOrRun(t) => return t.flatten(),
            JobResponse::EmptyJobs => {
                debug!("no more jobs to run on {node_meta} - sleeping and asking for more",);
                tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                continue;
            }
        }
    }
}

/// constantly polls a connection to ensure that
pub(crate) async fn complete_on_ping_failure(
    address: std::net::SocketAddr,
    node_meta: &pool_data::NodeMetadata,
) {
    loop {
        if let Err(e) = check_keepalive(&address, node_meta).await {
            error!(
                "error checking the keepalive for node at {}: {}",
                address, e
            );
            return;
        }

        #[cfg(not(test))]
        tokio::time::sleep(Duration::from_secs(6 * 60)).await;

        #[cfg(test)]
        tokio::time::sleep(Duration::from_secs(10)).await;
    }
}

/// ping keepalive address to ensure that the node responds within 10 seconds. If
/// the node does not respond, it means that it has silently gone offline and the
/// job needs to be returned to the scheduler to be allocated to a functioning node.
async fn check_keepalive(
    address: &std::net::SocketAddr,
    node_meta: &pool_data::NodeMetadata,
) -> Result<(), Error> {
    trace!("making keepalive check to {address} ({node_meta})");
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
        Err(_elapsed) => {
            Err(error::TimeoutError::new(*address, node_meta.node_name.to_string()).into())
        }
    }
}

/// monitor the receiver pipeline in the background and
///
/// this function must never return because there are `unreachable!()` calls
/// at the call sites.
pub(crate) async fn check_broadcast_for_matching_token(
    cancel_rx: &mut broadcast::Receiver<JobSetIdentifier>,
    cancel_address: &SocketAddr,
    job_id_to_monitor: JobSetIdentifier,
    cancelled_marker: &mut bool,
) -> ! {
    loop {
        while let Ok(job_id) = cancel_rx.recv().await {
            if job_id == job_id_to_monitor {
                let conn = tokio::net::TcpStream::connect(cancel_address).await;

                match conn {
                    Ok(_) => {
                        // if we successfully made the connection, that is all thats required
                        // for the compute node to know that the connection has been cancelled.
                        //
                        // See the corresponding compute implementation in
                        // `crate::client::return_on_cancellation`
                        *cancelled_marker = true;
                    }
                    Err(e) => {
                        error!("failed to connect to node's cancellation address {cancel_address}. It is currently running a job that should be cancelled. Error: {e}");
                        continue;
                    }
                };
            }
        }
    }
}
