use super::execute::FileMetadata;
use crate::error;
use futures::StreamExt;

use crate::prelude::*;

/// remove the directories up until "distribute_save" so that the file names that we send are
/// not absolute in their path - which makes saving things on the server side easier
pub(crate) fn remove_path_prefixes(path: PathBuf, distribute_save_path: &Path) -> PathBuf {
    path.strip_prefix(distribute_save_path).unwrap().to_owned()
}

pub(crate) fn read_folder_files(path: &Path) -> Vec<FileMetadata> {
    walkdir::WalkDir::new(path)
        .into_iter()
        .flat_map(|x| x.ok())
        .map(|x| FileMetadata {
            absolute_file_path: x.path().to_owned(),
            relative_file_path: remove_path_prefixes(x.path().to_owned(), path),
            is_file: x.file_type().is_file(),
        })
        .collect()
}

/// ensure that the `apptainer` executable is properly included in the $PATH
pub(crate) async fn verify_apptainer_in_path() -> Result<(), error::ExecutableMissing> {
    verify_executable_in_path("apptainer").await
}

/// ensure that a given executable is properly included in $PATH
async fn verify_executable_in_path(executable: &str) -> Result<(), error::ExecutableMissing> {
    let command = tokio::process::Command::new(executable).output().await;

    command
        .map(|_| ())
        .map_err(|e| error::ExecutableMissing::new(executable.into(), e))
}

#[derive(Debug, Clone, From, Display)]
#[display(fmt = "{}", "base.display()")]
pub(crate) struct WorkingDir {
    base: PathBuf,
}

impl WorkingDir {
    pub(crate) fn initial_files_folder(&self) -> PathBuf {
        self.base.join("initial_files")
    }

    pub(crate) fn input_folder(&self) -> PathBuf {
        self.base.join("input")
    }

    pub(crate) fn distribute_save_folder(&self) -> PathBuf {
        self.base.join("distribute_save")
    }

    pub(crate) async fn clean_input(&self) -> Result<(), std::io::Error> {
        debug!("running clear_input_files for {}", self.base.display());
        let path = self.input_folder();
        tokio::fs::remove_dir_all(&path).await?;
        tokio::fs::create_dir(&path).await?;

        debug!("removed and created ./input/ directory. Going to start copying form initial_files");

        let file_source = self.initial_files_folder();

        let read_dir = tokio::fs::read_dir(&file_source).await?;
        let mut stream = tokio_stream::wrappers::ReadDirStream::new(read_dir);

        while let Some(file) = stream.next().await {
            if let Ok(file) = file {
                debug!(
                    "handling the copying of file/ dir {}",
                    file.path().display()
                );

                // read the file type
                if let Ok(file_type) = file.file_type().await {
                    // check that it is a file
                    if file_type.is_file() {
                        let from = file.path();
                        let to = path.join(file.file_name());

                        debug!("copying {} to {}", from.display(), to.display());
                        tokio::fs::copy(from, to).await?;
                    } else {
                        continue;
                    }
                } else {
                    continue;
                }
            }
        }

        Ok(())
    }

    // clear all the files from the distribute_save directory
    pub(crate) async fn clean_distribute_save(&self) -> Result<(), std::io::Error> {
        let dist_save = self.distribute_save_folder();
        tokio::fs::remove_dir_all(&dist_save).await.ok();
        tokio::fs::create_dir(&dist_save).await?;

        Ok(())
    }

    /// clean out the tmp files from a build script from the output directory
    /// and recreate the distributed_save folder
    pub(crate) async fn delete_and_create_folders(&self) -> Result<(), error::CreateDir> {
        debug!("cleaning output folders and creating ./input ./initial_files ./distribute_save");

        tokio::fs::remove_dir_all(&self.base).await.ok();

        tokio::fs::create_dir(&self.base)
            .await
            .map_err(|e| error::CreateDir::new(e, self.base.to_owned()))?;

        let dist_save = self.distribute_save_folder();
        tokio::fs::create_dir(&dist_save)
            .await
            .map_err(|e| error::CreateDir::new(e, dist_save.to_owned()))?;

        let input = self.input_folder();
        tokio::fs::create_dir(&input)
            .await
            .map_err(|e| error::CreateDir::new(e, input.to_owned()))?;

        let initial_files = self.initial_files_folder();
        tokio::fs::create_dir(&initial_files)
            .await
            .map_err(|e| error::CreateDir::new(e, initial_files.to_owned()))?;

        Ok(())
    }

    pub(crate) fn exists(&self) -> bool {
        self.base.exists()
    }

    pub(crate) fn python_run_file(&self) -> PathBuf {
        self.base.join("run.py")
    }

    pub(crate) fn apptainer_sif(&self) -> PathBuf {
        self.base.join("apptainer.sif")
    }

    pub(crate) fn base(&self) -> &Path {
        &self.base
    }
}

/// other methods only used for testing purposes (and running the jobs locally)
impl WorkingDir {
    pub(crate) async fn copy_initial_files_apptainer(
        &self,
        init: &config::apptainer::Initialize<config::common::File>,
        state: &mut client::execute::BindingFolderState,
    ) {
        let initial_files = self.initial_files_folder();

        for file in init.required_files.iter() {
            let filename = file.filename().unwrap();
            std::fs::copy(file.path(), initial_files.join(filename)).unwrap();
        }

        std::fs::copy(init.sif.path(), self.apptainer_sif()).unwrap();

        state
            .update_binded_paths(init.required_mounts.clone(), self.base())
            .await;
    }

    pub(crate) async fn copy_job_files_apptainer(
        &self,
        job: &config::apptainer::Job<config::common::File>,
    ) {
        self.clean_input().await.unwrap();

        let input = self.input_folder();

        for file in job.required_files.iter() {
            let filename = file.filename().unwrap();
            std::fs::copy(file.path(), input.join(filename)).unwrap();
        }
    }

    pub(crate) async fn copy_job_files_python(
        &self,
        job: &config::python::Job<config::common::File>,
    ) {
        self.clean_input().await.unwrap();

        let input = self.input_folder();

        for file in job.required_files().iter() {
            let filename = file.filename().unwrap();
            std::fs::copy(file.path(), input.join(filename)).unwrap();
        }

        // copy the job file to the input directory, the python job runner will copy it to
        // the correct location
        std::fs::copy(
            job.python_job_file().path(),
            input.join(job.python_job_file().filename().unwrap()),
        )
        .unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn apptainer_exists() {
        verify_apptainer_in_path().await.unwrap();
    }

    #[tokio::test]
    #[should_panic]
    async fn apptainer_not_exists() {
        verify_executable_in_path("apptainer2").await.unwrap();
    }
}
