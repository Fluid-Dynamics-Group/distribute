use crate::{
    cli, config,
    error::{self, Error},
    transport,
};

/// check that all the nodes are up *and* the versions match. returns `true` if all nodes are
/// healthy w/ version matches
pub(crate) async fn status_command(status: cli::Status) -> Result<(), Error> {
    let nodes_config: config::Nodes = config::load_config(&status.node_information)?;

    status_check_nodes(&nodes_config.nodes).await?;

    Ok(())
}

pub(crate) async fn status_check_nodes(
    nodes: &[config::Node],
) -> Result<Vec<transport::ServerConnection>, Error> {
    let mut connections = vec![];
    let mut print_statements = vec![];

    for node in nodes {
        debug!("status check for ip {}", node.ip);
        let mut connection = transport::ServerConnection::new(node.addr()).await?;
        debug!("successfully connected to {}", node.ip);
        connection
            .transport_data(&transport::RequestFromServer::StatusCheck)
            .await?;

        debug!("status check sent to {}", node.ip);
        let response = connection.receive_data().await?;

        match response {
            transport::ClientResponse::StatusCheck(status_response) => {
                debug!("successful response recieved from {}", node.ip);
                print_statements.push(StatusCheckResponse::Good {
                    node,
                    version: status_response.version,
                    ready: status_response.ready,
                });
            }
            x => {
                print_statements.push(StatusCheckResponse::InvalidResponse { node, response: x });
            }
        }

        connections.push(connection);
    }

    let mut all_good = true;

    for stmt in print_statements {
        let out = stmt.print(transport::Version::current_version());
        all_good = all_good && out;
    }

    if all_good {
        Ok(connections)
    } else {
        Err(Error::Server(error::ServerError::MissingNode))
    }
}

pub(crate) enum StatusCheckResponse<'a> {
    Good {
        node: &'a config::Node,
        version: transport::Version,
        ready: bool,
    },
    InvalidResponse {
        node: &'a config::Node,
        response: transport::ClientResponse,
    },
}

impl<'a> StatusCheckResponse<'a> {
    pub(crate) fn print(&'a self, self_version: transport::Version) -> bool {
        match self {
            Self::Good {
                node,
                version,
                ready,
            } => {
                let version_match = self_version == *version;
                println!(
                    "ip {} version match {} ready {}",
                    node, version_match, ready
                );
                if !version_match {
                    dbg!(self_version, &version);
                }
                version_match
            }
            Self::InvalidResponse { node, response } => {
                println!("ip {} invalid status check response: {}", node, response);
                false
            }
        }
    }
}
