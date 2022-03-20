pub(crate) mod execute;
pub(crate) mod utils;

pub(crate) use execute::{
    initialize_python_job, initialize_singularity_job, run_python_job, run_singularity_job,
    BindingFolderState,
};

use crate::protocol;
use crate::prelude::*;

use crate::{cli, error, error::Error, transport};

use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;


pub async fn client_command(client: cli::Client) -> Result<(), Error> {
    let base_path = PathBuf::from(client.base_folder);
    utils::clean_output_dir(&base_path)
        .await
        .map_err(error::ClientInitError::from)?;

    let listener = TcpListener::bind(SocketAddr::from(([0, 0, 0, 0], client.port)))
        .await
        .map_err(error::TcpConnection::from)?;

    loop {
        let (tcp_conn, _address) = listener
            .accept()
            .await
            .map_err(error::TcpConnection::from)?;

        let uninit = protocol::Machine::new(tcp_conn);

    }

    #[allow(unreachable_code)]
    Ok(())
}

/// answer the server's message on the query port.
///
/// The message is a short query about what we are doing, including a keepalive
async fn answer_query_connection(client_conn: transport::Connection<transport::ClientQueryAnswer>) -> Result<(), error::TcpConnection> {
    todo!()
}

fn kill_job(tx_cancel: &mut broadcast::Sender<()>) {
    // try to send out the cancelation order to all children.
    // since this function is called when there is an actively running
    // job task. However, there is probably a race condition in there somewhere
    // so we just ignore this possible error
    tx_cancel.send(()).ok();
}

/// check if the error from reading transport data from the tcp connection
/// was due to the connection being closed - and if so it means
/// that we should mark ourselves ready for additional jobs
///
/// returns true if the connection has been closed
fn is_closed_connection(error: error::TcpConnection) -> bool {
    match error {
        // TODO: experiment with what exactly is the EOF on a TCP connection
        // and what exactly constitutes waiting for more data
        error::TcpConnection::ConnectionClosed => true,
        _ => false,
    }
}
