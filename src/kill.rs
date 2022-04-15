use crate::{cli, error::Error, transport};

use std::net::SocketAddr;

pub async fn kill(args: cli::Kill) -> Result<(), Error> {
    let addr = SocketAddr::from((args.ip, args.port));

    let mut conn = transport::Connection::new(addr).await?;

    conn.transport_data(&transport::UserMessageToServer::KillJob(args.job_name))
        .await?;

    let kill_job = match conn.receive_data().await {
        Ok(transport::ServerResponseToUser::KillJob(result)) => result,
        Ok(_x) => unreachable!(),
        Err(e) => Err(e)?,
    };

    println!("result of killing the job: {}", kill_job);

    Ok(())
}
