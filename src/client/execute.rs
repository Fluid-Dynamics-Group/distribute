use super::utils;
use super::EXEC_GROUP_ID;
use crate::{cli, error, error::Error, transport};

use std::net::SocketAddr;
use std::time::{Duration, Instant};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
use tokio::sync::mpsc;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// handle all branches of a request from the server
pub(super) async fn general_request(
    request: transport::RequestFromServer,
    base_path: &Path,
    cancel: &mut broadcast::Receiver<()>,
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

            initialize_job(init, base_path, cancel).await?;

            PrerequisiteOperations::None(transport::ClientResponse::RequestNewJob(
                transport::NewJobRequest,
            ))
        }
        transport::RequestFromServer::AssignJob(job) => {
            info!("running job request");

            if let Some(_) = run_job(job, base_path, cancel).await? {
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
            } else {
                // we cancelled this job early - dont send any files
                PrerequisiteOperations::DoNothing
            }
        }
        transport::RequestFromServer::FileReceived => {
            warn!("got a file recieved message from the server but we didnt send any files");
            PrerequisiteOperations::DoNothing
        }
        transport::RequestFromServer::KillJob => {
            warn!("got request to kill the job from the server but we dont have an actively running job");
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

/// execute a job after the build file has already been built
///
/// returns None if the job was cancelled
async fn run_job(
    job: transport::Job,
    base_path: &Path,
    cancel: &mut broadcast::Receiver<()>,
) -> Result<Option<transport::FinishedJob>, Error> {
    info!("running general job");

    let file_path = base_path.join("run.py");
    let mut file = tokio::fs::File::create(&file_path)
        .await
        .map_err(|full_error| error::RunJobError::CreateFile {
            full_error,
            path: file_path.clone(),
        })?;

    debug!("created run file");

    file.write_all(&job.python_file)
        .await
        .map_err(|full_error| error::RunJobError::WriteBytes {
            full_error,
            path: file_path.clone(),
        })?;

    debug!("wrote all job file bytes to file - running job");
    let original_dir = enter_output_dir(base_path);

    let command = tokio::process::Command::new("python3")
        .args(&["run.py", &num_cpus::get_physical().to_string()])
        .output();

    tokio::select!(
       output = command => {
           // command has finished -> return to the original dir so we dont accidentally
           // bubble the error up with `?` before we have fixed the directory
           enter_output_dir(&original_dir);

           let output = output
               .map_err(|e| error::CommandExecutionError::from(e))
               .map_err(|e| error::RunJobError::ExecuteProcess(e))?;

           debug!("job successfully finished - returning to main process");

           // write the stdout and stderr to a file
           let output_file_path = base_path.join(format!("distribute_save/{}_output.txt", job.job_name));
           command_output_to_file(output, output_file_path).await;

           Ok(Some(transport::FinishedJob))
       }
       _ = cancel.recv() => {
           info!("run_job has been canceled for job name {}", job.job_name);
           Ok(None)
       }
    )
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

/// run the build file for a job
///
/// returns None if the process was cancelled
async fn initialize_job(
    init: transport::JobInit,
    base_path: &Path,
    cancel: &mut broadcast::Receiver<()>,
) -> Result<Option<()>, Error> {
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

    let command = tokio::process::Command::new("python3")
        .args(&["run.py"])
        .output();

    tokio::select!(
        // this is the path for running the command and writing stuff to the files
        // etc
        output = command => {
            // return to original directory
            enter_output_dir(&original_dir);
            debug!("current file path is {:?}", std::env::current_dir());

            let output = output
                .map_err(|e| error::CommandExecutionError::from(e))
                .map_err(|e| error::RunJobError::ExecuteProcess(e))?;

            // write stdout / stderr to file
            let output_file_path = base_path.join(format!(
                "distribute_save/{}_init_output.txt",
                init.batch_name
            ));

            command_output_to_file(output, output_file_path).await;

            debug!("finished init command, returning to main process");

            Ok(Some(()))
        }
        // this branch allows for an early exit if a broadcast is received
        _ = cancel.recv() => {
           info!("initialize_job has been canceled for job name {}", init.batch_name);
           Ok(None)
        }
    )
}

fn enter_output_dir(base_path: &Path) -> PathBuf {
    debug!("entering path {}", base_path.display());
    let current_path = std::env::current_dir().unwrap();
    std::env::set_current_dir(base_path).unwrap();

    current_path
}

pub(super) enum PrerequisiteOperations {
    None(transport::ClientResponse),
    SendFiles {
        paths: Vec<FileMetadata>,
        after: transport::ClientResponse,
    },
    DoNothing,
}

async fn command_output_to_file(output: std::process::Output, path: PathBuf) {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    let output = format!("STDOUT:\n{}\nSTDERR:\n{}", stdout, stderr);

    let print_err = |e: std::io::Error| {
        warn!(
            "error writing stdout/stderr to txt file: {} - {}",
            e,
            path.display()
        )
    };

    match tokio::fs::File::create(&path).await {
        Ok(mut file) => {
            if let Err(e) = file.write_all(&output.as_bytes()).await {
                print_err(e)
            }
        }
        Err(e) => print_err(e),
    };
}
