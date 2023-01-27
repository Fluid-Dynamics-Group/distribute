use super::NormalizePaths;
use derive_more::Constructor;

use super::LoadJobsError;
use super::ReadBytesError;
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

#[cfg(feature = "cli")]
use crate::transport;

use getset::{Getters, MutGetters, Setters};

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
        //let sif_bytes = tokio::fs::read(&self.initialize.sif)
        //    .await
        //    .map_err(|e| ReadBytesError::new(e, self.initialize.sif.clone()))?;

        //let build_files = load_from_file(&self.initialize.required_files).await?;

        //Ok(transport::ApptainerJobInit {
        //    sif_bytes,
        //    batch_name,
        //    build_files,
        //    container_bind_paths: self.initialize.required_mounts.clone(),
        //})
    }

    pub(super) fn hashed(
        &self,
    ) -> Result<Description<common::HashedFile>, super::MissingFileNameError> {
        let initialize = self.initialize.hashed()?;

        let jobs = self
            .jobs
            .iter()
            .enumerate()
            .map(|(job_idx, job)| {
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

                // assemble the new files into a new job
                let job = Job {
                    name: job.name().to_string(),
                    required_files,
                };
                Ok(job)
            })
            .collect::<Result<Vec<_>, super::MissingFileNameError>>()?;

        let desc = Description { initialize, jobs };

        Ok(desc)
    }

    pub(super) fn sendable_files(
        &self,
        hashed: &Description<common::HashedFile>,
    ) -> Vec<FileMetadata> {
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

impl<FILE> NormalizePaths for Description<FILE>
where
    FILE: NormalizePaths,
{
    fn normalize_paths(&mut self, base: PathBuf) {
        // for initialize
        self.initialize.sif =
            super::common::normalize_pathbuf(self.initialize.sif.clone(), base.clone());
        for file in self.initialize.required_files.iter_mut() {
            file.normalize_paths(base.clone());
        }

        // for jobs
        for job in self.jobs.iter_mut() {
            for file in job.required_files.iter_mut() {
                file.normalize_paths(base.clone())
            }
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, Constructor)]
#[serde(deny_unknown_fields)]
pub struct Initialize<FILE> {
    pub sif: PathBuf,
    #[serde(default = "Default::default")]
    pub required_files: Vec<FILE>,
    /// paths in the folder that need to have mount points to
    /// the host file system
    #[serde(default)]
    pub required_mounts: Vec<PathBuf>,
}

#[cfg(feature = "cli")]
impl Initialize<common::File> {
    fn hashed(&self) -> Result<Initialize<common::HashedFile>, super::MissingFileNameError> {
        let init_hash = hashing::filename_hash(self);

        let path_string = format!("setup_sif_{init_hash}.dist");

        // hashed sif name
        let sif = PathBuf::from(path_string);

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
            sif,
            required_files,
            required_mounts: self.required_mounts.clone(),
        };

        Ok(init)
    }

    fn sendable_files(
        &self,
        hashed: &Initialize<common::HashedFile>,
        files: &mut Vec<FileMetadata>,
    ) {
        let sif = FileMetadata::file(&self.sif, &hashed.sif);
        files.push(sif);

        for (original_file, hashed_file) in
            self.required_files.iter().zip(hashed.required_files.iter())
        {
            let meta = FileMetadata::file(original_file.path(), hashed_file.hashed_filename());
            files.push(meta);
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, Constructor, Getters)]
#[serde(deny_unknown_fields)]
pub struct Job<FILE> {
    #[getset(get = "pub(crate)")]
    name: String,
    #[serde(default = "Default::default")]
    #[getset(get = "pub(crate)")]
    pub required_files: Vec<FILE>,
}
