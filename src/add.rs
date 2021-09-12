use crate::cli;
use crate::error::{self, Error};
use crate::transport;
use crate::server;
use crate::config;

use std::net::SocketAddr;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

pub(crate) async fn add(args: cli::Add) -> Result<(), Error> {
    //
    // load the config files
    //
    let jobs = config::load_config::<config::Jobs>(&args.jobs)?;

    debug!("loading job information from files");
    let loaded_jobs = jobs.load_jobs().await.map_err(error::ServerError::from)?;

    debug!("loading build information from files");
    let loaded_build = jobs.load_build().await.map_err(error::ServerError::from)?;

    //
    // check the server for all of the node capabilities
    //

    let addr = SocketAddr::from((args.ip, args.port));

    let mut conn = transport::UserConnectionToServer::new(addr).await?;

    conn.transport_data(&transport::UserMessageToServer::QueryCapabilities).await?;

    let caps = match conn.receive_data().await {
        Ok(transport::ServerResponseToUser::Capabilities(x)) => x,
        Ok(x) => {
            return Err(Error::from(error::AddError::NotCapabilities(x)))
        }
        Err(e) => Err(e)?
    };

    if args.show_caps {
        println!("all node capabilities:");

        for cap in &caps {
            println!("{}", cap);
        }
    }

    //
    // calculate how many of the nodes can run this command
    //

    let total_nodes = caps.len();
    let mut working_nodes = 0;

    for cap in &caps {
        if cap.can_accept_job(&jobs.init.capabilities) {
            working_nodes += 1;
        }
    }

    println!("these jobs can be run on {}/{} of the nodes", working_nodes, total_nodes);

    if working_nodes == 0 {
        return Err(error::AddError::NoCompatableNodes)?
    }


    //
    // construct the job set and send it off
    //

    let job_set = server::JobSet::new(loaded_build, jobs.init.capabilities, loaded_jobs, 0, jobs.init.batch_name);

    debug!("sending job set to server");
    conn.transport_data(&transport::UserMessageToServer::AddJobSet(job_set)).await?;
    
    match conn.receive_data().await {
        Ok(transport::ServerResponseToUser::JobSetAdded) => (),
        Ok(transport::ServerResponseToUser::JobSetAddedFailed) => Err(error::AddError::FailedToAdd)?,
        Ok(x) => {
            return Err(Error::from(error::AddError::NotCapabilities(x)))
        }
        Err(e) => Err(e)?
    };

    Ok(())
}