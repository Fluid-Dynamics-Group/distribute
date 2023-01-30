use super::NormalizePaths;

// event though these are included in the prelude, the prelude only exists for the cli
// feature
use derive_more::Constructor;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::common;

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
    pub(super) fn verify_config(&self) -> Result<(), super::MissingFileNameError> {
        self.initialize.python_build_file_path().exists_or_err()?;

        self.initialize
            .required_files
            .iter()
            .try_for_each(|f| f.exists_or_err())?;

        self.jobs.iter().try_for_each(|job| {
            job.required_files
                .iter()
                .try_for_each(|f| f.exists_or_err())
        })?;

        Ok(())
    }

    pub(crate) fn len_jobs(&self) -> usize {
        self.jobs.len()
    }

    pub(super) fn hashed(
        &self,
    ) -> Result<Description<common::HashedFile>, super::MissingFileNameError> {
        let initialize = self.initialize.hashed()?;

        let mut jobs = Vec::new();

        for (job_idx, job) in self.jobs.iter().enumerate() {
            let hashed_job = job.hashed(job_idx)?;

            jobs.push(hashed_job);
        }

        let desc = Description { initialize, jobs };

        Ok(desc)
    }
}

#[cfg(feature = "cli")]
impl Description<common::HashedFile> {
    pub(super) fn sendable_files(&self, is_user: bool) -> Vec<FileMetadata> {
        let mut files = Vec::new();

        self.initialize.sendable_files(is_user, &mut files);

        for job in self.jobs.iter() {
            job.sendable_files(is_user, &mut files);
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
        self.initialize
            .python_build_file_path
            .normalize_paths(base.clone());

        for file in self.initialize.required_files.iter_mut() {
            file.normalize_paths(base.clone());
        }

        // for jobs
        for job in self.jobs.iter_mut() {
            job.python_job_file.normalize_paths(base.clone());

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
    pub python_build_file_path: FILE,
    #[serde(default = "Default::default")]
    #[getset(get = "pub(crate)")]
    pub(super) required_files: Vec<FILE>,
}

#[cfg(feature = "cli")]
impl Initialize<common::File> {
    pub(crate) fn hashed(
        &self,
    ) -> Result<Initialize<common::HashedFile>, super::MissingFileNameError> {
        let init_hash = hashing::filename_hash(self);

        let hashed_path = format!("setup_python_{init_hash}.dist").into();
        let unhashed_path = self.python_build_file_path.path().into();
        let filename = self.python_build_file_path.filename()?;
        let python_build_file_path = common::HashedFile::new(hashed_path, unhashed_path, filename);

        // hashed required files
        let required_files = self
            .required_files
            .iter()
            .enumerate()
            .map(|(idx, init_file)| {
                let hashed_path = format!("{init_hash}_{idx}.dist").into();
                let unhashed_path = init_file.path().into();
                let filename = init_file.filename()?;
                Ok(common::HashedFile::new(
                    hashed_path,
                    unhashed_path,
                    filename,
                ))
            })
            .collect::<Result<Vec<_>, super::MissingFileNameError>>()?;

        let init = Initialize {
            python_build_file_path,
            required_files,
        };

        Ok(init)
    }
}

#[cfg(feature = "cli")]
impl Initialize<common::HashedFile> {
    pub(super) fn sendable_files(&self, is_user: bool, files: &mut Vec<FileMetadata>) {
        files.push(self.python_build_file_path.as_sendable(is_user));

        self.required_files
            .iter()
            .for_each(|hashed_file| files.push(hashed_file.as_sendable(is_user)));
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, Constructor, getset::Getters)]
#[serde(deny_unknown_fields)]
pub struct Job<FILE> {
    #[getset(get = "pub(crate)")]
    pub(super) name: String,
    #[serde(rename = "file")]
    #[getset(get = "pub(crate)")]
    pub(super) python_job_file: FILE,
    #[serde(default = "Default::default")]
    #[getset(get = "pub(crate)")]
    pub(super) required_files: Vec<FILE>,
}

#[cfg(feature = "cli")]
impl Job<common::File> {
    pub(crate) fn hashed(
        &self,
        job_idx: usize,
    ) -> Result<Job<common::HashedFile>, super::MissingFileNameError> {
        let hash = hashing::filename_hash(self);

        let required_files = self
            .required_files
            .iter()
            .enumerate()
            .map(|(file_idx, file)| {
                //
                // for each file in this job ...
                //

                let hashed_path = format!("{hash}_{job_idx}_{file_idx}.dist").into();
                let unhashed_path = file.path().into();
                let filename = file.filename()?;
                Ok(common::HashedFile::new(
                    hashed_path,
                    unhashed_path,
                    filename,
                ))
            })
            .collect::<Result<Vec<_>, super::MissingFileNameError>>()?;

        let hashed_path = format!("{hash}_python_run.dist").into();
        let unhashed_path = self.python_job_file.path().into();
        let filename = self.python_job_file.filename()?;
        let python_job_file = common::HashedFile::new(hashed_path, unhashed_path, filename);

        // assemble the new files into a new job
        let job = Job {
            name: self.name().to_string(),
            python_job_file,
            required_files,
        };

        Ok(job)
    }
}

#[cfg(feature = "cli")]
impl Job<common::HashedFile> {
    pub(crate) fn sendable_files(&self, is_user: bool, files: &mut Vec<FileMetadata>) {
        files.push(self.python_job_file.as_sendable(is_user));

        self.required_files
            .iter()
            .for_each(|hashed_file| files.push(hashed_file.as_sendable(is_user)));
    }
}
