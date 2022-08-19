use crate::{
    cli,
    error::{self, Error},
    transport,
};

use std::net::SocketAddr;

/// check that all the nodes are up *and* the versions match. returns `true` if all nodes are
/// healthy w/ version matches
pub async fn server_status(args: cli::ServerStatus) -> Result<(), Error> {
    let job_list = get_current_jobs(&args).await?;

    for batch in job_list {
        println!("{}", batch.batch_name);

        for job in batch.jobs_left {
            println!("\t-{}", job);
        }

        println!("\t:jobs running now: {}", batch.running_jobs);
    }

    Ok(())
}

pub async fn get_current_jobs(
    args: &cli::ServerStatus,
) -> Result<Vec<crate::server::RemainingJobs>, Error> {
    let addr = SocketAddr::from((args.ip, args.port));

    let mut conn = transport::Connection::new(addr).await?;

    conn.transport_data(&transport::UserMessageToServer::QueryJobNames)
        .await?;

    match conn.receive_data().await {
        Ok(transport::ServerResponseToUser::JobNames(x)) => Ok(x),
        Ok(x) => Err(Error::from(error::StatusError::NotQueryJobs(x))),
        Err(e) => Err(e).map_err(Error::from),
    }
}
