use super::utils;

use crate::{error, error::Error, transport};



use tokio::io::{AsyncWriteExt};

use tokio::sync::broadcast;


use std::path::{Path, PathBuf};



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
        transport::RequestFromServer::InitPythonJob(init) => {
            info!("running init python job request");

            utils::clean_output_dir(base_path)
                .await
                .map_err(|e| error::RemovePreviousDir::new(e, base_path.to_owned()))
                .map_err(error::InitJobError::from)?;

            initialize_python_job(init, base_path, cancel).await?;

            let after = transport::ClientResponse::RequestNewJob(transport::NewJobRequest);

            let paths = utils::read_save_folder(&base_path).await;

            PrerequisiteOperations::SendFiles { paths, after }
        }
        transport::RequestFromServer::RunPythonJob(job) => {
            info!("running python job");

            if let Some(_) = run_python_job(job, base_path, cancel).await? {
                let after = transport::ClientResponse::RequestNewJob(transport::NewJobRequest);
                let paths = utils::read_save_folder(base_path).await;
                PrerequisiteOperations::SendFiles { paths, after }
            } else {
                // we cancelled this job early - dont send any files
                PrerequisiteOperations::DoNothing
            }
        }
        transport::RequestFromServer::InitSingularityJob(init_job) => {
            info!("initializing a singularity job");

            utils::clean_output_dir(base_path)
                .await
                .map_err(|e| error::RemovePreviousDir::new(e, base_path.to_owned()))
                .map_err(error::InitJobError::from)?;

            initialize_singularity_job(init_job, base_path, cancel).await?;

            let after = transport::ClientResponse::RequestNewJob(transport::NewJobRequest);

            let paths = utils::read_save_folder(&base_path).await;

            PrerequisiteOperations::SendFiles { paths, after }
        }
        transport::RequestFromServer::RunSingularityJob(job) => {
            info!("running singularity job");

            if let Some(_) = run_singularity_job(job, base_path, cancel).await? {
                let after = transport::ClientResponse::RequestNewJob(transport::NewJobRequest);
                let paths = utils::read_save_folder(&base_path).await;

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

pub(crate) struct FileMetadata {
    pub file_path: PathBuf,
    pub is_file: bool,
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
async fn run_python_job(
    job: transport::PythonJob,
    base_path: &Path,
    cancel: &mut broadcast::Receiver<()>,
) -> Result<Option<()>, Error> {
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

    debug!("wrote bytes to run file");

    // reset the input files directory
    utils::clear_input_files(base_path)
        .await
        .map_err(|e| error::CreateDirError::new(e, base_path.to_owned()))
        .map_err(|e| error::RunJobError::CreateDir(e))?;

    // write all of _our_ job files to the output directory
    write_all_init_files(&base_path.join("input"), &job.job_files).await?;

    debug!("wrote all job file bytes to file - running job");
    let original_dir = enter_output_dir(base_path);

    let command = tokio::process::Command::new("python3")
        .args(&["run.py", &num_cpus::get_physical().to_string()])
        .output();

    let output_file_path = base_path.join(format!("distribute_save/{}_output.txt", job.job_name));

    command_with_cancellation(
        Some(&original_dir),
        command,
        output_file_path,
        &job.job_name,
        false,
        cancel,
    )
    .await
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
async fn initialize_python_job(
    init: transport::PythonJobInit,
    base_path: &Path,
    cancel: &mut broadcast::Receiver<()>,
) -> Result<Option<()>, Error> {
    info!("running initialization for new job");
    write_init_file(base_path, "run.py", &init.python_setup_file).await?;

    write_all_init_files(
        &base_path.join("initial_files"),
        &init.additional_build_files,
    )
    .await?;

    debug!("initialized all init files");

    // enter the file to execute the file from
    let original_dir = enter_output_dir(base_path);
    debug!("current file path is {:?}", std::env::current_dir());

    let command = tokio::process::Command::new("python3")
        .args(&["run.py"])
        .output();

    let output_file_path = base_path.join(format!(
        "distribute_save/{}_init_output.txt",
        init.batch_name
    ));

    command_with_cancellation(
        Some(&original_dir),
        command,
        output_file_path,
        &init.batch_name,
        true,
        cancel,
    )
    .await
}

async fn initialize_singularity_job(
    init: transport::SingularityJobInit,
    base_path: &Path,
    _cancel: &mut broadcast::Receiver<()>,
) -> Result<Option<()>, Error> {
    // write the .sif file to the root
    write_init_file(base_path, "singularity.sif", &init.sif_bytes).await?;
    // write any included files for the initialization to the `initial_files` directory
    // and they will be copied over to `input` at the start of each job run
    write_all_init_files(&base_path.join("initial_files"), &init.build_files).await?;

    // TODO: I think we can ignore the cancel signal here since after initializing we are going to
    // ask for a new job anyway
    Ok(Some(()))
}

/// execute a job after the build file has already been built
///
/// returns None if the job was cancelled
async fn run_singularity_job(
    job: transport::SingularityJob,
    base_path: &Path,
    cancel: &mut broadcast::Receiver<()>,
) -> Result<Option<()>, Error> {
    info!("running singularity job");

    // reset the input files directory
    utils::clear_input_files(base_path)
        .await
        .map_err(|e| error::CreateDirError::new(e, base_path.to_owned()))
        .map_err(|e| error::RunJobError::CreateDir(e))?;

    // copy all the files for this job to the directory
    write_all_init_files(&base_path.join("input"), &job.job_files).await?;

    // the local paths to the save and input directories
    let dist_save = base_path
        .join("distribute_save")
        .to_string_lossy()
        .to_string();
    let input = base_path.join("input").to_string_lossy().to_string();
    let work = base_path.join("work").to_string_lossy().to_string();

    tokio::fs::create_dir(&work).await.ok();

    let bind_arg = format!(
        "{}:{}:rw,{}:{}:rw,{}:{}:rw",
        dist_save, "/distribute_save", input, "/input", work, "/hit3d/src/output"
    );

    let singularity_path = base_path
        .join("singularity.sif")
        .to_string_lossy()
        .to_string();

    let command = tokio::process::Command::new("singularity")
        .args(&[
            "run",
            "--app",
            "distribute",
            "--bind",
            &bind_arg,
            &singularity_path,
            &num_cpus::get_physical().to_string(),
        ])
        .output();

    let output_file_path = base_path.join(format!("distribute_save/{}_output.txt", job.job_name));

    command_with_cancellation(
        None,
        command,
        output_file_path,
        &job.job_name,
        false,
        cancel,
    )
    .await
}
async fn write_all_init_files(base_path: &Path, files: &[transport::File]) -> Result<(), Error> {
    for additional_file in files {
        debug!(
            "init file {} number of bytes written: {}",
            additional_file.file_name,
            additional_file.file_bytes.len()
        );
        write_init_file(
            base_path,
            &additional_file.file_name,
            &additional_file.file_bytes,
        )
        .await?;
    }
    Ok(())
}

fn enter_output_dir(base_path: &Path) -> PathBuf {
    debug!("entering path {}", base_path.display());
    let current_path = std::env::current_dir().unwrap();
    std::env::set_current_dir(base_path).unwrap();

    current_path
}

/// run a future producing a command till completion while also
/// checking for a cancellation signal from the host
async fn command_with_cancellation(
    original_dir: Option<&Path>,
    command: impl std::future::Future<Output = Result<std::process::Output, std::io::Error>>,
    output_file_path: PathBuf,
    name: &str,
    is_job_init: bool,
    cancel: &mut broadcast::Receiver<()>,
) -> Result<Option<()>, Error> {
    tokio::select!(
       output = command => {
           // command has finished -> return to the original dir so we dont accidentally
           // bubble the error up with `?` before we have fixed the directory
           if let Some(original_dir) = original_dir {
               enter_output_dir(&original_dir);
           }
            debug!("current file path is {:?}", std::env::current_dir());

           let output = output
               .map_err(|e| error::CommandExecutionError::from(e))
               .map_err(|e| error::RunJobError::ExecuteProcess(e))?;

           debug!("job successfully finished - returning to main process");

           // write the stdout and stderr to a file
           command_output_to_file(output, output_file_path).await;

           Ok(Some(()))
       }
       _ = cancel.recv() => {
           if is_job_init {
               info!("initialize_job has been canceled for batch name {}", name);
           } else {
               info!("run_job has been canceled for job name {}", name);
           }
           Ok(None)
       }
    )
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
