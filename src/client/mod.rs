mod execute;
mod utils;

use execute::PrerequisiteOperations;

use crate::{cli, error, error::Error, transport};

use std::net::SocketAddr;
use std::time::{Duration, Instant};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub(crate) const EXEC_GROUP_ID: u32 = 999232;

pub(crate) async fn client_command(client: cli::Client) -> Result<(), Error> {
    let ready_for_job = Arc::new(AtomicBool::new(true));
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

        let base_path_clone = base_path.clone();
        let ready_for_job_clone = Arc::clone(&ready_for_job);

        // if we have no current jobs running ...
        if ready_for_job.load(Ordering::Relaxed) {
            debug!("received new TCP connection on port - we answer the connection");
            ready_for_job.swap(false, Ordering::Relaxed);
            tokio::task::spawn(async move {
                if let Err(e) =
                    start_server_connection(tcp_conn, base_path_clone, ready_for_job_clone).await
                {
                    error!("failure to respond to server connection: {}", e);
                }
            });
        }
        // we do have jobs running right now, so we read in the data and see if its a status check
        // request
        else {
            debug!("received new TCP connection, however we are busy");
            let mut client_conn = transport::ClientConnection::new(tcp_conn);
            match client_conn.receive_data().await {
                Ok(request) => {
                    match request {
                        transport::RequestFromServer::StatusCheck => {
                            let response = transport::StatusResponse::new(
                                transport::Version::current_version(),
                                ready_for_job.load(Ordering::Relaxed),
                            );
                            let wrapped_response = transport::ClientResponse::StatusCheck(response);

                            if let Err(e) = client_conn.transport_data(&wrapped_response).await {
                                error!("could not send status check response to client on main thread (currently busy): {}",e);
                            }

                            //
                        }
                        _ => {
                            // TODO: log that we have gotten a real job request even though we are marked as
                            // not-ready
                            client_conn
                                .transport_data(&transport::ClientResponse::Error(
                                    transport::ClientError::NotReady,
                                ))
                                .await
                                .ok();
                        }
                    }
                }
                Err(e) => {
                    //
                    error!("error when reading socket: {}", e);
                }
            }
        }
    }

    #[allow(unreachable_code)]
    Ok(())
}

async fn start_server_connection(
    tcp_conn: TcpStream,
    base_path: PathBuf,
    ready_for_job: Arc<AtomicBool>,
) -> Result<(), Error> {
    let mut conn = transport::ClientConnection::new(tcp_conn);

    'main_loop: loop {
        let new_data = conn.receive_data().await;
        if let Ok(request) = new_data {
            info!("executing general request handler from main task");
            let result_response = execute::general_request(request, &base_path).await;

            // make sure the request from the client was actually handled correctly
            if let Ok(prereq_client_response) = result_response {
                match prereq_client_response {
                    PrerequisiteOperations::None(client_response) => {
                        send_client_response_with_logging(
                            client_response,
                            &mut conn,
                            &ready_for_job,
                            &base_path,
                        )
                        .await?;
                    }
                    PrerequisiteOperations::SendFiles { paths, after } => {
                        debug!("there are {} files to send to the server", paths.len());

                        // start by writing all of the file bytes to the tcp stream
                        // TODO: this has potential to allocate too much memory depending on how
                        // fast the network connection is at exporting and clearing information
                        // from memory
                        for metadata in paths.into_iter().skip(1) {
                            // remove leading directories up until (and including) distribute_save

                            match metadata.into_send_file() {
                                Ok(mut send_file) => {
                                    send_file.file_path =
                                        utils::remove_path_prefixes(send_file.file_path);

                                    let response = transport::ClientResponse::SendFile(send_file);
                                    send_client_response_with_logging(
                                        response,
                                        &mut conn,
                                        &ready_for_job,
                                        &base_path,
                                    )
                                    .await?;

                                    if let Ok(transport::RequestFromServer::FileReceived) =
                                        conn.receive_data().await
                                    {
                                        //
                                    } else {
                                        // TODO: handle this error better - perhaps with a receive
                                        // function to automatically do the swapping on an error
                                        error!("repsonse from file send was not Ok(FileReceived) - this should not happen. Terminating the connection");
                                        ready_for_job.swap(true, Ordering::Relaxed);
                                        break 'main_loop;
                                    }
                                }
                                Err(e) => {
                                    error!("could not read the bytes from the `send back` output files 
                                           - therefore this data has been moved by another process. Ths should not happen. {}", e);
                                }
                            }
                        }

                        debug!("all file transfers have finished - now sending new job request to server");

                        // now that we have sent all of the files out, we now send the `after`
                        // response
                        send_client_response_with_logging(
                            after,
                            &mut conn,
                            &ready_for_job,
                            &base_path,
                        )
                        .await?;
                    }
                    PrerequisiteOperations::DoNothing => {}
                }
            // we had an error at some part of the job / initialization
            // therefore, we need to just request a new job anyway
            } else if let Err(e) = result_response {
                // TODO: reject more jobs here becasue the build process did not complete correctly

                error!(
                    "could not build project, sending response to server for new job (FIXME). {}",
                    e
                );
                let new_job = transport::ClientResponse::RequestNewJob(transport::NewJobRequest);

                send_client_response_with_logging(new_job, &mut conn, &ready_for_job, &base_path)
                    .await?;
            }
        } else if let Err(e) = new_data {
            // we have not received anything on the tcp connection yet
            //
            // this probably means that we have no additional jobs available
            // from the node and

            if is_closed_connection(e) {
                debug!("connection has been closed, marking ourselves as ready for new jobs");
                ready_for_job.swap(true, Ordering::Relaxed);
                break;
            } else {
                std::thread::sleep(std::time::Duration::from_millis(1000));
                continue;
            }
        }
    }

    Ok(())
}

/// check if the error from reading transport data from the tcp connection
/// was due to the connection being closed - and if so it means
/// that we should mark ourselves ready for additional jobs
///
/// returns true if the connection has been closed
fn is_closed_connection(error: Error) -> bool {
    match error {
        // TODO: experiment with what exactly is the EOF on a TCP connection
        // and what exactly constitutes waiting for more data
        Error::TcpConnection(error::TcpConnection::ConnectionClosed) => true,
        _ => false,
    }
}

/// send off a message to the connection and mark ourselves as ready for additional jobs
/// if there was an error in the TCP connection
async fn send_client_response_with_logging(
    response: transport::ClientResponse,
    conn: &mut transport::ClientConnection,
    ready_for_job: &AtomicBool,
    base_save: &Path,
) -> Result<(), Error> {
    // if there is an error writing the response to the socket then we make sure
    // that we mark this node as ready for additional jobs
    //
    // this should really never happen unless the server node has been killed
    // in which case we dont have anything to do anyway
    if let Err(e) = conn.transport_data(&response).await {
        debug!("error sending client response to server, marking ourselves ready for a new job");
        ready_for_job.swap(true, Ordering::Relaxed);
        utils::clean_output_dir(base_save)
            .await
            .map_err(error::ClientInitError::from)?;
        Err(e)
    } else {
        Ok(())
    }
}
