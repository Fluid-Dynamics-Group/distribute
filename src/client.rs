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

pub async fn client_command(client: cli::Client) -> Result<(), Error> {
    let ready_for_job = Arc::new(AtomicBool::new(true));
    let base_path = PathBuf::from(client.base_folder);
    clean_output_dir(&base_path)
        .await
        .map_err(error::ClientInitError::from)?;

    let listener = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], client.port)))
        .await
        .map_err(error::TcpConnection::from)?;

    // create an arbiter that will pause and unpause the processes created by this program
    let (pause_execution_arbiter, pause_tx) = PauseProcessArbiter::new();
    pause_execution_arbiter.spawn();

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
                        transport::RequestFromServer::PauseExecution(pause_data) => {
                            // send the pause duration to the arbiter
                            if let Err(e) =
                                pause_tx.send(Some(Instant::now() + pause_data.duration))
                            {
                                error!("Error sending data to the pausing arbiter task. full error: {}", e);
                            }
                        }
                        transport::RequestFromServer::ResumeExecution(_resume_data) => {
                            // send the pause duration to the arbiter
                            if let Err(e) = pause_tx.send(None) {
                                error!("Error sending data to the pausing arbiter task. full error: {}", e);
                            }
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
    loop {
        let new_data = conn.receive_data().await;
        if let Ok(request) = new_data {
            info!("executing general request handler from main task");
            let result_response = execute_general_request(request, &base_path).await;

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
                        // start by writing all of the file bytes to the tcp stream
                        // TODO: this has potential to allocate too much memory depending on how
                        // fast the network connection is at exporting and clearing information
                        // from memory
                        for metadata in paths {
                            match metadata.into_send_file() {
                                Ok(send_file) => {
                                    let response = transport::ClientResponse::SendFile(send_file);
                                    send_client_response_with_logging(
                                        response,
                                        &mut conn,
                                        &ready_for_job,
                                        &base_path,
                                    )
                                    .await?;
                                }
                                Err(e) => {
                                    error!("could not read the bytes from the `send back` output files 
                                           - therefore this data has been moved by another process. Ths should not happen. {}", e);
                                }
                            }
                        }

                        debug!("all file transfers have finished - now sending new job request to server");

                        // TODO: log that all the file transfers have finished

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

enum PrerequisiteOperations {
    None(transport::ClientResponse),
    SendFiles {
        paths: Vec<FileMetadata>,
        after: transport::ClientResponse,
    },
    DoNothing,
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
        clean_output_dir(base_save)
            .await
            .map_err(error::ClientInitError::from)?;
        Err(e)
    } else {
        Ok(())
    }
}

/// handle all branches of a request from the server
async fn execute_general_request(
    request: transport::RequestFromServer,
    base_path: &Path,
) -> Result<PrerequisiteOperations, Error> {
    let output = match request {
        transport::RequestFromServer::StatusCheck => {
            info!("running status check request");
            // if we have been spawned off into this thread it means we are not doing anything
            // right now which means that `ready` is true
            let response =
                transport::StatusResponse::new(transport::Version::current_version(), true);
            PrerequisiteOperations::None(transport::ClientResponse::StatusCheck(response))
        }
        transport::RequestFromServer::AssignJobInit(init) => {
            info!("running init job request");
            clean_output_dir(base_path)
                .await
                .map_err(|e| error::RemovePreviousDir::new(e, base_path.to_owned()))
                .map_err(error::InitJobError::from)?;
            //
            initialize_job(init, base_path).await?;
            PrerequisiteOperations::None(transport::ClientResponse::RequestNewJob(
                transport::NewJobRequest,
            ))
        }
        transport::RequestFromServer::AssignJob(job) => {
            info!("running job request");
            //
            run_job(job, base_path).await?;
            let after = transport::ClientResponse::RequestNewJob(transport::NewJobRequest);
            let paths = walkdir::WalkDir::new(base_path.join("output"))
                .into_iter()
                .flat_map(|x| x.ok())
                .map(|x| FileMetadata {
                    file_path: x.path().to_owned(),
                    is_file: x.file_type().is_file(),
                })
                .collect();
            PrerequisiteOperations::SendFiles { paths, after }
        }
        transport::RequestFromServer::PauseExecution(_) => {
            info!("received request to pause the execution of the process - however the main thread picked up this request which means there are no commands currently running. ignoring the request");
            PrerequisiteOperations::DoNothing
        }
        transport::RequestFromServer::ResumeExecution(_) => {
            info!("received request to resume the execution of the process - however the main thread picked up this request which means there are no commands currently running. ignoring the request");
            PrerequisiteOperations::DoNothing
        }
    };

    Ok(output)
}

struct FileMetadata {
    file_path: PathBuf,
    is_file: bool,
}

impl FileMetadata {
    fn into_send_file(self) -> Result<transport::SendFile, Error> {
        let Self { file_path, is_file } = self;

        // if its a file read the bytes, otherwise skip it
        let bytes = if is_file {
            std::fs::read(&file_path).map_err(|e| error::RunJobError::ReadBytes {
                path: file_path.to_owned(),
                full_error: e,
            })?
        } else {
            vec![]
        };

        Ok(transport::SendFile {
            file_path,
            is_file,
            bytes,
        })
    }
}

async fn run_job(job: transport::Job, base_path: &Path) -> Result<transport::FinishedJob, Error> {
    info!("running general job");

    let file_path = base_path.join("run.py");
    let mut file = tokio::fs::File::create(&file_path)
        .await
        .map_err(|full_error| error::RunJobError::CreateFile {
            full_error,
            path: file_path.clone(),
        })?;

    debug!("created run file");

    file.write(&job.python_file)
        .await
        .map_err(|full_error| error::RunJobError::WriteBytes {
            full_error,
            path: file_path.clone(),
        })?;

    debug!("wrote all job file bytes to file - running job");
    let original_dir = enter_output_dir(base_path);

    let output = tokio::process::Command::new("python3")
        .args(&["run.py"])
        .gid(EXEC_GROUP_ID)
        .output()
        .await
        .map_err(|e| error::CommandExecutionError::from(e))
        .map_err(|e| error::RunJobError::ExecuteProcess(e))?;

    enter_output_dir(&original_dir);

    debug!("job successfully finished - returning to main process");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    println!("stdout: \n{}", stdout);
    println!("stderr: \n{}", stderr);
    Ok(transport::FinishedJob)
}

async fn write_init_file<T: AsRef<Path>>(
    base_path: &Path,
    file_name: T,
    bytes: &[u8],
) -> Result<(), error::InitJobError> {
    let file_path = base_path.join(file_name);

    debug!("creating file {} for job init", file_path.display());

    let mut file = tokio::fs::File::create(&file_path)
        .await
        .map_err(error::InitJobError::from)?;

    file.write(bytes).await.map_err(error::InitJobError::from)?;

    Ok(())
}

async fn initialize_job(init: transport::JobInit, base_path: &Path) -> Result<(), Error> {
    info!("running initialization for new job");
    write_init_file(base_path, "run.py", &init.python_setup_file).await?;

    for additional_file in init.additional_build_files {
        write_init_file(
            base_path,
            additional_file.file_name,
            &additional_file.file_bytes,
        )
        .await?;
    }

    debug!("initialized all init files");

    // enter the file to execute the file from
    let original_dir = enter_output_dir(base_path);

    let output = tokio::process::Command::new("python3")
        .args(&["run.py"])
        .gid(EXEC_GROUP_ID)
        .output()
        .await
        .map_err(|e| error::CommandExecutionError::from(e))
        .map_err(|e| error::RunJobError::ExecuteProcess(e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    println!("stdout: \n{}", stdout);
    println!("stderr: \n{}", stderr);

    debug!("finished init command, returning to main process");

    // return to original directory
    enter_output_dir(&original_dir);

    Ok(())
}

fn enter_output_dir(base_path: &Path) -> PathBuf {
    debug!("entering path {}", base_path.display());
    let current_path = std::env::current_dir().unwrap();
    std::env::set_current_dir(base_path).unwrap();

    current_path
}

async fn clean_output_dir(dir: &Path) -> Result<(), std::io::Error> {
    tokio::fs::remove_dir_all(dir).await.ok();

    tokio::fs::create_dir(dir).await?;
    tokio::fs::create_dir(dir.join("distribute_save")).await?;

    Ok(())
}

struct PauseProcessArbiter {
    unpause_instant: Option<Instant>,
    rx: std::sync::mpsc::Receiver<Option<Instant>>,
}

impl PauseProcessArbiter {
    /// Sending a None unpauses the execution
    /// Sending a Some(instant) will pause the underlying process until
    /// that instant
    fn new() -> (Self, std::sync::mpsc::Sender<Option<Instant>>) {
        // we use std channels here because there is no easy way to check
        // if there is a value in the `Receiver` with tokio channels
        let (tx, rx) = std::sync::mpsc::channel();
        (
            Self {
                unpause_instant: None,
                rx,
            },
            tx,
        )
    }

    fn spawn(mut self) -> tokio::task::JoinHandle<()> {
        tokio::task::spawn(async move {
            if let Ok(sleep_update) = self.rx.try_recv() {
                match sleep_update {
                    // request to set a pause time in the future, we pause now
                    Some(future_instant) => {
                        self.unpause_instant = Some(future_instant);
                        self.pause_execution();
                    }
                    // resume right away
                    None => {
                        self.unpause_execution();
                    }
                }
            }

            if let Some(instant) = self.unpause_instant {
                if Instant::now() > instant {
                    self.unpause_execution();
                    self.unpause_instant = None;
                }
            }

            tokio::time::sleep(Duration::from_secs(10)).await;
        })
    }

    // pause the execution of all processes using the specified groupid
    fn pause_execution(&self) {
        let signal = nix::sys::signal::Signal::SIGSTOP;
        let process_id = nix::unistd::Pid::from_raw(EXEC_GROUP_ID as i32);
        if let Err(e) = nix::sys::signal::kill(process_id, signal) {
            error!(
                "error when pausing group process (id {}): {}",
                EXEC_GROUP_ID, e
            );
        }
    }

    // pause the execution of all processes using the specified groupid
    fn unpause_execution(&self) {
        let signal = nix::sys::signal::Signal::SIGCONT;
        let process_id = nix::unistd::Pid::from_raw(EXEC_GROUP_ID as i32);
        if let Err(e) = nix::sys::signal::kill(process_id, signal) {
            error!(
                "error when resuming group process (id {}): {}",
                EXEC_GROUP_ID, e
            );
        }
    }
}
