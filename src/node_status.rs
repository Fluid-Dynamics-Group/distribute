use crate::cli;
use crate::prelude::*;
use crate::transport::{self, Connection, Version};
use error::Error;

#[derive(Constructor)]
struct StatusResponse {
    name: String,
    address: SocketAddr,
    status: Status,
}

impl StatusResponse {
    fn pretty_print(&self) {
        println!("{} {} {}", self.name, self.address, self.status);
    }
}

#[derive(From, Display)]
enum Status {
    #[display(fmt = "{}", _0)]
    Online(Version),
    #[display(fmt = "OFFLINE")]
    Offline,
}

pub async fn node_status(args: cli::NodeStatus) -> Result<(), Error> {
    let nodes_config = config::load_config::<config::Nodes>(&args.nodes_file)?;

    let mut results = Vec::new();

    for node in nodes_config.nodes {
        let c = tokio::net::TcpStream::connect(node.keepalive_addr())
            .await
            .map_err(|e| {
                results.push(StatusResponse::new(
                    node.node_name.clone(),
                    node.keepalive_addr(),
                    Status::Offline,
                ));
                error::TcpConnection::from(e)
            })?;
        let mut connection: Connection<transport::ServerQuery> = Connection::from_connection(c);

        connection
            .transport_data(&transport::ServerQuery::VersionCheck)
            .await
            .map_err(error::TcpConnection::from)?;

        match connection.receive_data().await.unwrap() {
            transport::ClientQueryAnswer::KeepaliveResponse => unreachable!(),
            transport::ClientQueryAnswer::VersionResponse(version) => {
                results.push(StatusResponse::new(
                    node.node_name.clone(),
                    node.keepalive_addr(),
                    Status::from(version),
                ));
            }
        }
    }

    println!("Node Name  |   Node Address   | Node Status");
    for r in results {
        r.pretty_print()
    }

    Ok(())
}
