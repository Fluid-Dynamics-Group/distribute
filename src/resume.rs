use crate::{
    cli,
    error::{self, Error},
    transport,
};
use std::net::{Ipv4Addr, SocketAddr};

pub async fn resume(args: cli::Resume) -> Result<(), Error> {
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, args.port));

    // emulate a server connection here since the host client process
    // only expects messsages from a "server"
    let mut conn = transport::ServerConnection::new(addr).await?;

    //let request = transport::ResumeExecution::new();
    //let wrapped_request = transport::RequestFromServer::from(request);
    //conn.transport_data(&wrapped_request).await?;

    Ok(())
}
