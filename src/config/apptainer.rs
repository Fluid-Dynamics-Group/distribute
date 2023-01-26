use super::NormalizePaths;
use derive_more::Constructor;

use super::LoadJobsError;
use super::ReadBytesError;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[cfg(feature = "cli")]
use super::common::load_from_file;
use super::common::File;

#[cfg(feature = "cli")]
use crate::client::execute::FileMetadata;

#[cfg(feature = "cli")]
use crate::transport;

use getset::{Getters, Setters, MutGetters};

#[derive(Debug, Clone, Deserialize, Serialize, Constructor, getset::Getters)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "python", pyo3::pyclass)]
pub struct Description {
    #[getset(get = "pub(crate)")]
    pub initialize: Initialize,
    #[getset(get = "pub(crate)")]
    pub jobs: Vec<Job>,
}

#[cfg(feature = "cli")]
impl Description {
    pub(crate) fn len_jobs(&self) -> usize {
        self.jobs.len()
    }

    pub(crate) async fn jobset_files(&self) -> Result<Vec<FileMetadata>, LoadJobsError> {
        todo!()
        //let mut out = Vec::with_capacity(self.jobs.len());

        //for job in &self.jobs {
        //    for file in required_files {
        //        let job = transport::ApptainerJob {
        //            job_name: job.name.clone(),
        //            job_files,
        //        };

        //        let file = FileMetadata {
        //            is_file: true,
        //            absolute_file_path: 
        //            relative_file_path: 
        //        };

        //        out.push(job)
        //    }
        //}

        //Ok(out)
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
}

impl NormalizePaths for Description {
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
#[cfg_attr(feature = "python", pyo3::pyclass)]
pub struct Initialize {
    pub sif: PathBuf,
    #[serde(default)]
    pub required_files: Vec<File>,
    /// paths in the folder that need to have mount points to
    /// the host file system
    #[serde(default)]
    pub required_mounts: Vec<PathBuf>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Constructor, getset::Getters)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "python", pyo3::pyclass)]
pub struct Job {
    #[getset(get = "pub(crate)")]
    name: String,
    #[serde(default)]
    #[getset(get = "pub(crate)")]
    pub required_files: Vec<File>,
}
