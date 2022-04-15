mod job_pool;
mod matrix;
pub(crate) mod node;
mod user_conn;

pub(crate) mod pool_data;
mod schedule;
mod storage;

use crate::protocol::Common;
use job_pool::JobPool;
pub(crate) use pool_data::JobRequest;

pub use pool_data::CancelResult;
pub(crate) use schedule::{JobIdentifier, RemainingJobs};
pub(crate) use storage::OwnedJobSet;

use crate::{cli, config, error, error::Error};
use crate::prelude::*;

#[cfg(feature = "cli")]
use tokio::sync::{broadcast, mpsc};

#[cfg(feature = "cli")]
pub async fn server_command(server: cli::Server) -> Result<(), Error> {
    debug!("starting server");

    let nodes_config = config::load_config::<config::Nodes>(&server.nodes_file)?;
    debug!("finished loading nodes config");

    if server.save_path.exists() && server.clean_output {
        std::fs::remove_dir_all(&server.save_path).map_err(|e| {
            error::ServerError::from(error::RemoveDirError::new(e, server.save_path.clone()))
        })?;
    }

    ok_if_exists(tokio::fs::create_dir_all(&server.save_path).await).map_err(|e| {
        error::ServerError::from(error::CreateDir::new(e, server.save_path.clone()))
    })?;

    debug!("creating an output folder for the server");

    // make the output folder - if it already exits then dont error
    match std::fs::create_dir_all(&server.save_path) {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(e) => Err(error::ServerError::from(error::CreateDir::new(
            e,
            server.save_path.clone(),
        ))),
    }?;

    let node_caps = nodes_config
        .nodes
        .iter()
        .map(|node| std::sync::Arc::new(node.capabilities.clone()))
        .collect::<Vec<_>>();

    // communication pipes with the scheduler
    let (request_job, job_pool_holder) = mpsc::channel(100);

    // spawn off a connection to listen to user requests
    let port = server.port;
    let req_clone = request_job.clone();
    let caps_clone = node_caps.clone();
    let save_path = server.save_path.clone();
    tokio::spawn(async move {
        user_conn::handle_user_requests(port, req_clone, caps_clone, save_path).await;
    });

    // spawn off a job pool that we can query from different tasks

    let scheduler =
        schedule::GpuPriority::new(Default::default(), Default::default(), server.temp_dir);

    info!("starting job pool task");
    let (tx_cancel, _) = broadcast::channel(20);
    let job_pool_handle = JobPool::new(
        scheduler,
        job_pool_holder,
        tx_cancel.clone(),
        node_caps.len(),
    )
    .spawn();

    let mut handles = vec![job_pool_handle];

    // spawn off each node connection to its own task
    for node in nodes_config.nodes {
        let keepalive_addr = node.keepalive_addr();
        let transport_addr = node.transport_addr();
        let common = Common::new(
            tx_cancel.subscribe(),
            Arc::new(node.capabilities),
            server.save_path.clone(),
            node.node_name,
            keepalive_addr,
            transport_addr,
            // btreeset is just empty
            Default::default()
        );

        let request_job_tx = request_job.clone();

        let fut = tokio::spawn(async move {
            node::run_node(common, request_job_tx).await;
        });

        handles.push(fut);
    }

    // join the futures here so that the process will not end early
    futures::future::join_all(handles).await;

    Ok(())
}

#[cfg(feature = "cli")]
pub(crate) fn ok_if_exists(x: Result<(), std::io::Error>) -> Result<(), std::io::Error> {
    match x {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(e) => Err(e),
    }?;

    Ok(())
}
