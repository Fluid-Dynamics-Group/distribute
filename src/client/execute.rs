use crate::{cli, error, error::Error, transport};
use super::EXEC_GROUP_ID;
use super::utils;

use std::net::SocketAddr;
use std::time::{Duration, Instant};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// handle all branches of a request from the server
pub(super) async fn general_request(
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

            utils::clean_output_dir(base_path)
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
            let paths = walkdir::WalkDir::new(base_path.join("distribute_save"))
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

pub(super) struct FileMetadata {
    file_path: PathBuf,
    is_file: bool,
}

impl FileMetadata {
    pub(super) fn into_send_file(self) -> Result<transport::SendFile, Error> {
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

    file.write_all(bytes)
        .await
        .map_err(error::InitJobError::from)?;

    Ok(())
}

async fn initialize_job(init: transport::JobInit, base_path: &Path) -> Result<(), Error> {
    info!("running initialization for new job");
    write_init_file(base_path, "run.py", &init.python_setup_file).await?;

    for additional_file in init.additional_build_files {
        debug!(
            "init file {} number of bytes written: {}",
            additional_file.file_name,
            additional_file.file_bytes.len()
        );
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

    debug!("current file path is {:?}", std::env::current_dir());

    let output = tokio::process::Command::new("python3")
        .args(&["run.py"])
        //.gid(EXEC_GROUP_ID)
        //.env_clear()
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
    debug!("current file path is {:?}", std::env::current_dir());

    Ok(())
}

fn enter_output_dir(base_path: &Path) -> PathBuf {
    debug!("entering path {}", base_path.display());
    let current_path = std::env::current_dir().unwrap();
    std::env::set_current_dir(base_path).unwrap();

    current_path
}


pub(super) struct PauseProcessArbiter {
    unpause_instant: Option<Instant>,
    rx: std::sync::mpsc::Receiver<Option<Instant>>,
}

impl PauseProcessArbiter {
    /// Sending a None unpauses the execution
    /// Sending a Some(instant) will pause the underlying process until
    /// that instant
    pub(super) fn new() -> (Self, std::sync::mpsc::Sender<Option<Instant>>) {
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

    pub(super) fn spawn(mut self) -> tokio::task::JoinHandle<()> {
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

pub(super) enum PrerequisiteOperations {
    None(transport::ClientResponse),
    SendFiles {
        paths: Vec<FileMetadata>,
        after: transport::ClientResponse,
    },
    DoNothing,
}
