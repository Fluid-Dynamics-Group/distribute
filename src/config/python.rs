use super::LoadJobsError;
use super::NormalizePaths;
use super::ReadBytesError;

#[cfg(feature = "cli")]
use crate::prelude::*;

// event though these are included in the prelude, the prelude only exists for the cli
// feature
use derive_more::Constructor;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::common;
#[cfg(feature = "cli")]
use super::common::load_from_file;
use super::common::File;

#[cfg(feature = "cli")]
use super::hashing;

#[cfg(feature = "cli")]
use crate::client::execute::FileMetadata;

use getset::Getters;

#[derive(Debug, Clone, Deserialize, Serialize, Constructor, Getters)]
#[serde(deny_unknown_fields)]
pub struct Description<FILE> {
    #[getset(get = "pub(crate)")]
    pub initialize: Initialize<FILE>,
    #[getset(get = "pub(crate)")]
    pub jobs: Vec<Job<FILE>>,
}

#[cfg(feature = "cli")]
impl Description<common::File> {
    pub(crate) fn len_jobs(&self) -> usize {
        self.jobs.len()
    }

    pub(crate) async fn load_build(
        &self,
        batch_name: String,
    ) -> Result<Vec<FileMetadata>, LoadJobsError> {
        todo!()
        //let bytes = tokio::fs::read(&self.initialize.python_build_file_path)
        //    .await
        //    .map_err(|e| ReadBytesError::new(e, self.initialize.python_build_file_path.clone()))?;

        //let additional_build_files = load_from_file(&self.initialize.required_files).await?;

        //debug!(
        //    "number of initial files included: {}",
        //    additional_build_files.len()
        //);

        //Ok(transport::PythonJobInit {
        //    batch_name,
        //    python_setup_file: bytes,
        //    additional_build_files,
        //})
    }

    pub(super) fn hashed(
        &self,
    ) -> Result<Description<common::HashedFile>, super::MissingFileNameError> {
        let initialize = self.initialize.hashed()?;

        let mut jobs = Vec::new();

        for (job_idx, job) in self.jobs.iter().enumerate() {
            //
            // for each job ...
            //

            let hash = hashing::filename_hash(job);

            let required_files = job
                .required_files
                .iter()
                .enumerate()
                .map(|(file_idx, file)| {
                    //
                    // for each file in this job ...
                    //

                    let hashed_filename = format!("{hash}_{job_idx}_{file_idx}.dist").into();
                    let filename = PathBuf::from(file.filename()?);

                    Ok(common::HashedFile::new(hashed_filename, filename))
                })
                .collect::<Result<Vec<_>, super::MissingFileNameError>>()?;

            let python_job_file = format!("{hash}_python_run.dist").into();

            // assemble the new files into a new job
            let job = Job {
                name: job.name().to_string(),
                python_job_file,
                required_files,
            };

            jobs.push(job);
        }

        let desc = Description { initialize, jobs };

        Ok(desc)
    }

    pub(super) fn sendable_files(&self, hashed: &Description<common::HashedFile>) -> Vec<FileMetadata> {
        let mut files = Vec::new();

        self.initialize
            .sendable_files(&hashed.initialize, &mut files);

        for (original_job, hashed_job) in self.jobs.iter().zip(hashed.jobs().iter()) {
            let file_iter = original_job
                .required_files
                .iter()
                .zip(hashed_job.required_files.iter());

            for (original_file, hashed_file) in file_iter {
                let meta = FileMetadata::file(original_file.path(), hashed_file.hashed_filename());
                files.push(meta);
            }
        }

        files
    }
}

impl NormalizePaths for Description<common::File> {
    fn normalize_paths(&mut self, base: PathBuf) {
        // for initialize
        self.initialize.python_build_file_path = super::common::normalize_pathbuf(
            self.initialize.python_build_file_path.clone(),
            base.clone(),
        );
        for file in self.initialize.required_files.iter_mut() {
            file.normalize_paths(base.clone());
        }

        // for jobs
        for job in self.jobs.iter_mut() {
            job.python_job_file =
                super::common::normalize_pathbuf(job.python_job_file.clone(), base.clone());

            for file in job.required_files.iter_mut() {
                file.normalize_paths(base.clone())
            }
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, Constructor, getset::Getters)]
#[serde(deny_unknown_fields)]
pub struct Initialize<FILE> {
    #[serde(rename = "build_file")]
    #[getset(get = "pub(crate)")]
    pub python_build_file_path: PathBuf,
    #[serde(default = "Default::default")]
    #[getset(get = "pub(crate)")]
    required_files: Vec<FILE>,
}

#[cfg(feature="cli")]
impl Initialize<common::File> {
    fn hashed(&self) -> Result<Initialize<common::HashedFile>, super::MissingFileNameError> {
        let init_hash = hashing::filename_hash(self);

        let path_string = format!("setup_python_{init_hash}.dist");

        // hashed sif name
        let python_build_file_path = PathBuf::from(path_string);

        // hashed required files
        let required_files = self
            .required_files
            .iter()
            .enumerate()
            .map(|(idx, init_file)| {
                let path_string = format!("{init_hash}_{idx}.dist");
                let filename = init_file.filename()?;
                Ok(common::HashedFile::new(path_string, filename.into()))
            })
            .collect::<Result<Vec<_>, super::MissingFileNameError>>()?;

        let init = Initialize {
            python_build_file_path,
            required_files,
        };

        Ok(init)
    }

    pub(crate) fn sendable_files(
        &self,
        hashed: &Initialize<common::HashedFile>,
        files: &mut Vec<FileMetadata>,
    ) {
        let py_build =
            FileMetadata::file(&self.python_build_file_path, &hashed.python_build_file_path);
        files.push(py_build);

        for (original_file, hashed_file) in
            self.required_files.iter().zip(hashed.required_files.iter())
        {
            let meta = FileMetadata::file(original_file.path(), hashed_file.hashed_filename());
            files.push(meta);
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, Constructor, getset::Getters)]
#[serde(deny_unknown_fields)]
pub struct Job<FILE> {
    #[getset(get = "pub(crate)")]
    name: String,
    #[serde(rename = "file")]
    #[getset(get = "pub(crate)")]
    python_job_file: PathBuf,
    #[serde(default = "Default::default")]
    #[getset(get = "pub(crate)")]
    required_files: Vec<FILE>,
}
