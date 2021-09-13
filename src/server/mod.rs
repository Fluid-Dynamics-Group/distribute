mod job_pool;
mod schedule;

pub(crate) use job_pool::JobResponse;
use job_pool::{JobPool, JobRequest, NodeConnection};
pub(crate) use schedule::{
    JobRequiredCaps, JobSet, NodeProvidedCaps, Requirement, Requirements, Schedule,
};

use crate::{cli, config, error, error::Error, status, transport};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

pub(crate) async fn server_command(server: cli::Server) -> Result<(), Error> {
    let nodes = config::load_config::<config::Nodes>(&server.nodes_file)?;

    if server.save_path.exists() && server.clean_output {
        std::fs::remove_dir_all(&server.save_path).map_err(|e| {
            error::ServerError::from(error::RemoveDirError::new(e, server.save_path.clone()))
        })?;
    }

    ok_if_exists(tokio::fs::create_dir_all(&server.save_path).await).map_err(|e| {
        error::ServerError::from(error::CreateDirError::new(e, server.save_path.clone()))
    })?;

    // make the output folder - if it already exits then dont error
    match std::fs::create_dir_all(&server.save_path) {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(e) => Err(error::ServerError::from(error::CreateDirError::new(
            e,
            server.save_path.clone(),
        ))),
    }?;

    // start by checking the status of each node - if one of the nodes is not ready
    // then something is wrong

    log::info!("checking connection status of each node");
    let connections = status::status_check_nodes(&nodes.nodes).await?;

    let node_caps = nodes
        .nodes
        .into_iter()
        .map(|node| std::sync::Arc::new(node.capabilities))
        .collect::<Vec<_>>();

    let (request_job, job_pool_holder) = mpsc::channel(100);

    // spawn off a connection to listen to user requests
    let port = server.port;
    let req_clone = request_job.clone();
    let caps_clone = node_caps.clone();
    tokio::spawn(async move {
        handle_user_requests(port, req_clone, caps_clone).await;
    });

    // spawn off a job pool that we can query from different tasks

    let scheduler = schedule::GpuPriority::default();

    info!("starting job pool task");
    let handle = JobPool::new(scheduler, job_pool_holder).spawn();

    let mut handles = vec![handle];

    // spawn off each node connection to its own task
    for (server_connection, caps) in connections.into_iter().zip(node_caps.into_iter()) {
        info!("starting NodeConnection for {}", server_connection.addr);
        let handle = NodeConnection::new(
            server_connection,
            request_job.clone(),
            caps,
            server.save_path.clone(),
            None,
        )
        .spawn();
        handles.push(handle);
    }

    futures::future::join_all(handles).await;

    Ok(())
}

/// handle incomming requests from the user over CLI on any node
async fn handle_user_requests(
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
                debug!("the new request was AddJobSet");

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
                        .map(|_| {
                            debug!("alterted that the client that the job set could not be added")
                        })
                        // we errored when trying to tell the user about it
                        // this should probably not happen unless the client aborted the connection
                        .map_err(|e| {
                            error!("could not alert the client that the job was added successfully: {}", e)
                        })
                        .ok();
                }
            }
            transport::UserMessageToServer::QueryCapabilities => {
                debug!("the new request was QueryCapabilities");
                // clone all the data so that we have non-Arc'd data
                // this can be circumvented by
                let caps: Vec<Requirements<_>> =
                    node_capabilities.iter().map(|x| (**x).clone()).collect();

                if let Err(e) = conn
                    .transport_data(&transport::ServerResponseToUser::Capabilities(caps))
                    .await
                {
                    error!("error sending caps to user (this should not happen): {}", e)
                }
            }
        }
        //
    }
}

pub(crate) fn ok_if_exists(x: Result<(), std::io::Error>) -> Result<(), std::io::Error> {
    match x {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(e) => Err(e),
    }?;

    Ok(())
}
