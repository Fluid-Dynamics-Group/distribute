use crate::{cli, config, error, error::Error, status, transport};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use crate::server::job_pool::CancelBatchQuery;

use super::{schedule, JobRequest, NodeProvidedCaps, Requirements};

/// handle incomming requests from the user over CLI on any node
pub(crate) async fn handle_user_requests(
    port: u16,
    tx: mpsc::Sender<JobRequest>,
    node_capabilities: Vec<Arc<Requirements<NodeProvidedCaps>>>,
) {
    let listener = TcpListener::bind(SocketAddr::from(([0, 0, 0, 0], port)))
        .await
        .map_err(error::TcpConnection::from)
        .unwrap();

    loop {
        let (tcp_conn, address) = listener
            .accept()
            .await
            .map_err(error::TcpConnection::from)
            .unwrap();

        info!("new user connection from {}", address);

        let conn = transport::ServerConnectionToUser::new(tcp_conn);

        let tx_c = tx.clone();
        let node_c = node_capabilities.clone();

        tokio::spawn(async move {
            single_user_request(conn, tx_c, node_c).await;
            info!("user connection has closed");
        });
    }
}

// TODO: add user message to end the connection
// so that this function does not hang forever and generate extra tasks
async fn single_user_request(
    mut conn: transport::ServerConnectionToUser,
    tx: mpsc::Sender<JobRequest>,
    node_capabilities: Vec<Arc<Requirements<NodeProvidedCaps>>>,
) {
    loop {
        let request = match conn.receive_data().await {
            Ok(req) => {
                info!("a user request has been received");
                req
            }
            Err(e) => {
                error!("error reading user request: {}", e);
                return;
            }
        };

        match request {
            transport::UserMessageToServer::AddJobSet(set) => {
                add_job_set(&tx, set, &mut conn).await;
                debug!("the new request was AddJobSet");
            }
            transport::UserMessageToServer::QueryCapabilities => {
                debug!("the new request was QueryCapabilities");
                query_capabilities(&mut conn, &node_capabilities).await;
            }
            transport::UserMessageToServer::QueryJobNames => {
                debug!("the new request was to query the job names");
                query_job_names(&tx, &mut conn).await;
            }
            transport::UserMessageToServer::KillJob(batch_name) => {
                debug!("new request was to kill a job set");
                // TODO: ask the scheduler
                cancel_job_by_name(&tx, &mut conn, batch_name).await;
            }
        }
        //
    }
}

async fn add_job_set(
    tx: &mpsc::Sender<JobRequest>,
    set: super::OwnedJobSet,
    conn: &mut transport::ServerConnectionToUser,
) {
    // the output of sending the message to the server
    let tx_result = tx.send(JobRequest::AddJobSet(set)).await.map_err(|e| {
        error!(
            "error sending job set to pool (this should not happen): {}",
            e
        );
    });

    // if we were able to add the job set to the scheduler
    if let Ok(_) = tx_result {
        if let Err(e) = conn
            .transport_data(&transport::ServerResponseToUser::JobSetAdded)
            .await
        {
            error!(
                "could not respond to the user that the job set was added: {}",
                e
            );
        } else {
            debug!("alerted the client that the job set was added");
        }
    }
    // we were NOT able to schedule the job
    else {
        debug!("job set was successfully added to the server");

        conn.transport_data(&transport::ServerResponseToUser::JobSetAddedFailed)
            .await
            // we told the  user about it
            .map(|_| debug!("alterted that the client that the job set could not be added"))
            // we errored when trying to tell the user about it
            // this should probably not happen unless the client aborted the connection
            .map_err(|e| {
                error!(
                    "could not alert the client that the job was added successfully: {}",
                    e
                )
            })
            .ok();
    }
}

async fn query_capabilities(
    conn: &mut transport::ServerConnectionToUser,
    node_capabilities: &Vec<Arc<Requirements<NodeProvidedCaps>>>,
) {
    // clone all the data so that we have non-Arc'd data
    // this can be circumvented by
    let caps: Vec<Requirements<_>> = node_capabilities.iter().map(|x| (**x).clone()).collect();

    if let Err(e) = conn
        .transport_data(&transport::ServerResponseToUser::Capabilities(caps))
        .await
    {
        error!("error sending caps to user (this should not happen): {}", e)
    }
}

async fn query_job_names(
    tx: &mpsc::Sender<JobRequest>,
    conn: &mut transport::ServerConnectionToUser,
) {
    let (tx_respond, rx_respond) = oneshot::channel();

    if let Err(e) = tx
        .send(JobRequest::QueryRemainingJobs(
            super::job_pool::RemainingJobsQuery::new(tx_respond),
        ))
        .await
    {
        error!(
            "could not send message to job pool for `query_job_names`. This should not happen. {}",
            e
        );

        let response = transport::ServerResponseToUser::JobNamesFailed;
        if let Err(e) = conn.transport_data(&response).await {
            error!(
                "failed to respond to the user with failure to query job names: {}",
                e
            );
        }
    }

    match rx_respond.await {
        Ok(remaining_jobs) => {
            let response = transport::ServerResponseToUser::JobNames(remaining_jobs);

            if let Err(e) = conn.transport_data(&response).await {
                error!("failed to respond to the user with the job names: {}", e);
            }
        }
        Err(e) => {
            error!(
                "job pool did not respond over the oneshot channel. This should not happen: {:?}",
                e
            );

            let response = transport::ServerResponseToUser::JobNamesFailed;

            if let Err(e) = conn.transport_data(&response).await {
                error!("failed to respond to the user with failure to query job names (caused by oneshot channel): {}", e);
            }
        }
    }
}

async fn cancel_job_by_name(
    tx: &mpsc::Sender<JobRequest>,
    conn: &mut transport::ServerConnectionToUser,
    batch: String,
) {
    let (tx_respond, rx_respond) = oneshot::channel();

    let req = JobRequest::CancelBatchByName(CancelBatchQuery::new(tx_respond, batch.clone()));

    if let Err(e) = tx.send(req).await {
        error!("could not query job pool to remove a job set {} - {}", batch, e);
        let response = transport::ServerResponseToUser::KillJobFailed;
        conn.transport_data(&response).await.ok();
    }

    match rx_respond.await {
        Ok(result) => {
            let resp = transport::ServerResponseToUser::KillJob(result);
            conn.transport_data(&resp).await.ok();
        }
        Err(e) => {
            error!("could not read from oneshot pipe when getting killed job result");
            let response = transport::ServerResponseToUser::KillJobFailed;
            conn.transport_data(&response).await.ok();
        }
    }
}
