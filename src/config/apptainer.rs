use super::NormalizePaths;
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
/// initialization and job specification for apptainer job batches
pub struct Description<FILE> {
    #[getset(get = "pub(crate)")]
    /// initialization information on apptainer job
    pub initialize: Initialize<FILE>,
    #[getset(get = "pub(crate)")]
    /// list of jobs to execute in the job batch
    pub jobs: Vec<Job<FILE>>,
}

#[cfg(feature = "cli")]
impl Description<common::File> {
    pub(super) fn verify_config(&self) -> Result<(), super::MissingFilename> {
        self.initialize.sif.exists_or_err()?;

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

    pub(super) fn hashed(&self) -> Result<Description<common::HashedFile>, super::MissingFilename> {
        let initialize = self.initialize.hashed()?;

        let jobs = self
            .jobs
            .iter()
            .enumerate()
            .map(|(job_idx, job)| job.hashed(job_idx))
            .collect::<Result<Vec<_>, super::MissingFilename>>()?;

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
        self.initialize.sif.normalize_paths(base.clone());

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

#[derive(Debug, Clone, Deserialize, Serialize, Constructor, getset::Getters)]
#[serde(deny_unknown_fields)]
/// initialization information for python jobs
pub struct Initialize<FILE> {
    #[getset(get = "pub(crate)")]
    /// path to .sif file generated from apptainer
    pub sif: FILE,
    #[serde(default = "Default::default")]
    #[getset(get = "pub(crate)")]
    /// list of required files used in apptainer initialization. Generally, for apptainer jobs,
    /// these are simply files that will always be present
    pub required_files: Vec<FILE>,
    /// paths in the folder that need to have mount points to
    /// the host file system
    #[serde(default = "Default::default")]
    #[getset(get = "pub(crate)")]
    pub required_mounts: Vec<PathBuf>,
}

#[cfg(feature = "cli")]
impl Initialize<common::File> {
    fn hashed(&self) -> Result<Initialize<common::HashedFile>, super::MissingFilename> {
        let init_hash = hashing::filename_hash(self);

        // hash the sif
        let hashed_path = format!("setup_sif_{init_hash}.dist").into();
        let unhashed_path = self.sif.path().into();
        let filename = self.sif.filename()?;
        let sif = common::HashedFile::new(hashed_path, unhashed_path, filename);

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
            .collect::<Result<Vec<_>, super::MissingFilename>>()?;

        let init = Initialize {
            sif,
            required_files,
            required_mounts: self.required_mounts.clone(),
        };

        Ok(init)
    }
}

#[cfg(feature = "cli")]
impl Initialize<common::HashedFile> {
    pub(super) fn sendable_files(&self, is_user: bool, files: &mut Vec<FileMetadata>) {
        files.push(self.sif.as_sendable(is_user));

        self.required_files
            .iter()
            .for_each(|hashed_file| files.push(hashed_file.as_sendable(is_user)));
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, Constructor, Getters)]
#[serde(deny_unknown_fields)]
/// specification for an apptainer job
pub struct Job<FILE> {
    #[getset(get = "pub(crate)")]
    name: String,
    #[serde(default = "Default::default")]
    #[getset(get = "pub(crate)")]
    /// files required for the execution of this job
    pub required_files: Vec<FILE>,
    /// slurm configuration. This level of information will override the defeaults at the outer
    /// most level of the configuration
    #[getset(get = "pub(crate)")]
    slurm: Option<super::Slurm>,
}

#[cfg(feature = "cli")]
impl Job<common::File> {
    pub(crate) fn hashed(
        &self,
        job_idx: usize,
    ) -> Result<Job<common::HashedFile>, super::MissingFilename> {
        //
        // for each job ...
        //
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
            .collect::<Result<Vec<_>, super::MissingFilename>>()?;

        // assemble the new files into a new job
        let job = Job {
            name: self.name().to_string(),
            required_files,
            slurm: self.slurm.clone(),
        };

        Ok(job)
    }
}
#[cfg(feature = "cli")]
impl Job<common::HashedFile> {
    pub(crate) fn sendable_files(&self, is_user: bool, files: &mut Vec<FileMetadata>) {
        self.required_files
            .iter()
            .for_each(|hashed_file| files.push(hashed_file.as_sendable(is_user)));
    }
}
