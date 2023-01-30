use crate::prelude::*;
use error::Error;

use std::fmt::Write as _;

#[derive(Debug)]
pub(crate) struct BindingFolderState {
    counter: usize,
    folders: Vec<BindedFolder>,
}

impl BindingFolderState {
    pub(crate) fn new() -> Self {
        Self {
            counter: 0,
            folders: vec![],
        }
    }

    // TODO: decide if these should be hard errors and return Result< , >
    pub(super) async fn update_binded_paths(
        &mut self,
        container_paths: Vec<PathBuf>,
        base_path: &Path,
    ) {
        // first, clear out all the older folder bindings
        self.clear_folders();

        for container_path in container_paths.into_iter() {
            let host_path = base_path.join(format!("_bind_path_{}", self.counter));

            if host_path.exists() {
                Self::remove_dir_with_logging(&host_path);
            }

            if let Err(e) = tokio::fs::create_dir(&host_path).await {
                tracing::error!("failed to create a directory for the host FS bindings. This will create errors in the future: {}", e);
            }

            self.counter += 1;
            let new_bind = BindedFolder {
                host_path,
                container_path,
            };
            self.folders.push(new_bind)
        }
    }

    /// removes full directory of files that are being used for bindings
    fn clear_folders(&mut self) {
        for folder in self.folders.drain(..) {
            Self::remove_dir_with_logging(&folder.host_path)
        }
    }

    // this function cannot be async because we also use it in the Drop impl
    fn remove_dir_with_logging(path: &Path) {
        if let Err(e) = std::fs::remove_dir_all(path) {
            tracing::error!(
                "failed to remove old container path binding at host path {} - error: {}",
                path.display(),
                e
            );
        }
    }
}

impl std::ops::Drop for BindingFolderState {
    fn drop(&mut self) {
        trace!("executing Drop for BindingFolderState - removing all folders");
        self.clear_folders();
    }
}

/// describes the mapping from a host FS to a container FS
#[derive(Debug)]
pub(crate) struct BindedFolder {
    pub(crate) host_path: PathBuf,
    pub(crate) container_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct FileMetadata {
    pub absolute_file_path: PathBuf,
    pub relative_file_path: PathBuf,
    pub is_file: bool,
}

impl FileMetadata {
    pub(crate) fn into_send_file(self) -> Result<transport::SendFile, error::ReadBytes> {
        let Self {
            absolute_file_path,
            is_file,
            relative_file_path,
        } = self;

        // if its a file read the bytes, otherwise skip it
        let bytes = if is_file {
            std::fs::read(&absolute_file_path)
                .map_err(|e| error::ReadBytes::new(e, absolute_file_path.to_owned()))?
        } else {
            vec![]
        };

        Ok(transport::SendFile {
            file_path: relative_file_path,
            is_file,
            bytes,
        })
    }

    pub(crate) fn file<T: AsRef<Path>, U: AsRef<Path>>(
        absolute_file_path: T,
        relative_file_path: U,
    ) -> Self {
        let absolute_file_path = absolute_file_path.as_ref().to_owned();
        let relative_file_path = relative_file_path.as_ref().to_owned();

        FileMetadata {
            absolute_file_path,
            relative_file_path,
            is_file: true,
        }
    }
}

/// execute a job after the build file has already been built
///
/// returns None if the job was cancelled
pub(crate) async fn run_python_job(
    job: config::python::Job<config::common::HashedFile>,
    base_path: &WorkingDir,
    cancel: &mut broadcast::Receiver<()>,
) -> Result<Option<()>, Error> {
    info!("running general job");

    let file_path = base_path.python_run_file();
    let input_files = base_path.input_folder();

    let src = input_files.join(job.python_job_file().original_filename());
    let dest = file_path;
    std::fs::rename(&src, &dest)
        .map_err(|e| error::RenameFile::new(e, src, dest))
        .map_err(error::RunJobError::from)?;

    debug!("wrote bytes to run file");

    // all job files were written to ./base_path/input in the previous state machine
    // so we do not need to write them to the folder here

    debug!("wrote all job file bytes to file - running job");
    let original_dir = enter_output_dir(base_path.base());

    let command = tokio::process::Command::new("python3")
        .args(["run.py", &num_cpus::get_physical().to_string()])
        .output();

    let output_file_path = base_path
        .distribute_save_folder()
        .join(format!("{}_output.txt", job.name()));

    generalized_run(
        Some(&original_dir),
        command,
        output_file_path,
        &job.name(),
        cancel,
    )
    .await
}

/// run the build file for a job
///
/// returns None if the process was cancelled
pub(crate) async fn initialize_python_job(
    init: &config::python::Initialize<config::common::HashedFile>,
    batch_name: &str,
    base_path: &WorkingDir,
) -> Result<(), Error> {
    info!("running initialization for new job");

    let initial_files = base_path.initial_files_folder();

    let src = initial_files.join(init.python_build_file_path.original_filename());
    let dest = base_path.python_run_file();
    std::fs::rename(&src, &dest)
        .map_err(|e| error::RenameFile::new(e, src, dest))
        .map_err(error::ClientInitError::from)?;

    info!("initialized all init files");

    // enter the file to execute the file from
    let original_dir = enter_output_dir(&base_path.base());
    debug!("current file path is {:?}", std::env::current_dir());

    let command = tokio::process::Command::new("python3")
        .args(["run.py"])
        .output();

    let output_file_path = base_path
        .distribute_save_folder()
        .join(format!("{batch_name}_init_output.txt",));

    generalized_init(&original_dir, command, output_file_path).await
}

pub(crate) async fn initialize_apptainer_job(
    init: &config::apptainer::Initialize<config::common::HashedFile>,
    base_path: &WorkingDir,
    folder_state: &mut BindingFolderState,
) -> Result<(), Error> {
    let initial_files = base_path.initial_files_folder();

    let src = initial_files.join(init.sif.original_filename());
    let dest = base_path.apptainer_sif();
    std::fs::rename(&src, &dest)
        .map_err(|e| error::RenameFile::new(e, src, dest))
        .map_err(error::ClientInitError::from)?;

    // clear out all the older bindings and create new folders for our mounts
    // for this container
    folder_state
        .update_binded_paths(init.required_mounts.clone(), base_path.base())
        .await;
    Ok(())
}

/// execute a job after the build file has already been built
///
/// returns None if the job was cancelled
pub(crate) async fn run_apptainer_job(
    job: config::apptainer::Job<config::common::HashedFile>,
    base_path: &WorkingDir,
    cancel: &mut broadcast::Receiver<()>,
    folder_state: &BindingFolderState,
) -> Result<Option<()>, Error> {
    info!("running apptainer job");

    // all job files were written to ./base_path/input in the previous state machine
    // so we do not need to write them to the folder here

    let apptainer_path = base_path.apptainer_sif().to_string_lossy().to_string();

    let bind_arg = create_bind_argument(base_path, folder_state);

    info!("binding argument for apptainer job is {}", bind_arg);

    let mut command = tokio::process::Command::new("apptainer");
    command.args([
        "run",
        "--nv",
        "--app",
        "distribute",
        "--bind",
        &bind_arg,
        &apptainer_path,
        &num_cpus::get_physical().to_string(),
    ]);

    debug!("command to be run: {:?}", command);

    let command_output = command.output();

    let output_file_path = base_path
        .distribute_save_folder()
        .join(format!("{}_output.txt", job.name()));

    debug!("starting generalized run");

    generalized_run(None, command_output, output_file_path, &job.name(), cancel).await
}

/// create a --bind argument for `apptainer run`
fn create_bind_argument(base_path: &WorkingDir, folder_state: &BindingFolderState) -> String {
    let dist_save = base_path.distribute_save_folder();
    let input = base_path.input_folder();

    let mut bind_arg = format!(
        "{}:{}:rw,{}:{}:rw",
        dist_save.display(),
        "/distribute_save",
        input.display(),
        "/input"
    );

    // add bindings for any use-requested folders
    for folder in &folder_state.folders {
        // we know that we have previous folders
        // so we can always add a comma
        bind_arg.push(',');

        write!(
            bind_arg,
            "{}:{}:rw",
            folder.host_path.display(),
            folder.container_path.display()
        )
        .unwrap();
    }

    bind_arg
}

fn enter_output_dir(base_path: &Path) -> PathBuf {
    debug!("entering path {}", base_path.display());
    let current_path = std::env::current_dir().unwrap();
    std::env::set_current_dir(base_path).unwrap();

    current_path
}

/// run a future producing a command till completion while also
/// checking for a cancellation signal from the host
async fn generalized_init(
    original_dir: &Path,
    command: impl std::future::Future<Output = Result<std::process::Output, std::io::Error>>,
    output_file_path: PathBuf,
) -> Result<(), Error> {
    let output = command.await;

    // command has finished -> return to the original dir so we dont accidentally
    // bubble the error up with `?` before we have fixed the directory
    enter_output_dir(original_dir);

    debug!("current file path is {:?}", std::env::current_dir());

    let output = output
        .map_err(error::CommandExecutionError::from)
        .map_err(error::RunJobError::ExecuteProcess)?;

    debug!("job successfully finished - returning to main process");

    // write the stdout and stderr to a file
    command_output_to_file(output, output_file_path).await;

    Ok(())
}

/// run a future producing a command till completion while also
/// checking for a cancellation signal from the host
async fn generalized_run(
    original_dir: Option<&Path>,
    command: impl std::future::Future<Output = Result<std::process::Output, std::io::Error>>,
    output_file_path: PathBuf,
    name: &str,
    cancel: &mut broadcast::Receiver<()>,
) -> Result<Option<()>, Error> {
    debug!("started generalized_run()");
    tokio::select!(
        output = command => {
            debug!("started executing tokio::select!() for job execution");

            // command has finished -> return to the original dir so we dont accidentally
            // bubble the error up with `?` before we have fixed the directory
            if let Some(original_dir) = original_dir {
                enter_output_dir(original_dir);
            }
            debug!("current file path is {:?}", std::env::current_dir());

            let output = output
                .map_err(error::CommandExecutionError::from)
                .map_err(error::RunJobError::ExecuteProcess)?;

            debug!("job successfully finished - returning to main process");

            // write the stdout and stderr to a file
            command_output_to_file(output, output_file_path).await;

            Ok(Some(()))
       }
       _ = cancel.recv() => {
            if let Some(original_dir) = original_dir {
                enter_output_dir(original_dir);
            }

            info!("run_job has been canceled for job name {}", name);
            Ok(None)
       }
    )
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
            if let Err(e) = file.write_all(output.as_bytes()).await {
                print_err(e)
            }
        }
        Err(e) => print_err(e),
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bind_arg_1() {
        let base_path = WorkingDir::from(PathBuf::from("/"));
        let state = BindingFolderState::new();
        let out = create_bind_argument(&base_path, &state);
        assert_eq!(out, "/distribute_save:/distribute_save:rw,/input:/input:rw");
    }

    #[tokio::test]
    async fn bind_arg_2() {
        let base_path = WorkingDir::from(PathBuf::from("/some/"));

        let mut state = BindingFolderState::new();
        state
            .update_binded_paths(vec![PathBuf::from("/reqpath")], &base_path.base())
            .await;

        let out = create_bind_argument(&base_path, &state);
        assert_eq!(out, "/some/distribute_save:/distribute_save:rw,/some/input:/input:rw,/some/_bind_path_0:/reqpath:rw");
    }
}
