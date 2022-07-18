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
use crate::transport;

#[derive(Debug, Clone, Deserialize, Serialize, Constructor)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "python", pyo3::pyclass)]
pub struct Description {
    pub initialize: Initialize,
    pub jobs: Vec<Job>,
}

#[cfg(feature = "cli")]
impl Description {
    pub(crate) fn len_jobs(&self) -> usize {
        self.jobs.len()
    }

    pub(crate) async fn load_jobs(&self) -> Result<Vec<transport::ApptainerJob>, LoadJobsError> {
        let mut out = Vec::with_capacity(self.jobs.len());

        for job in &self.jobs {
            let job_files = load_from_file(&job.required_files).await?;

            let job = transport::ApptainerJob {
                job_name: job.name.clone(),
                job_files,
            };
            out.push(job)
        }

        Ok(out)
    }

    pub(crate) async fn load_build(
        &self,
        batch_name: String,
    ) -> Result<transport::ApptainerJobInit, LoadJobsError> {
        let sif_bytes = tokio::fs::read(&self.initialize.sif)
            .await
            .map_err(|e| ReadBytesError::new(e, self.initialize.sif.clone()))?;

        let build_files = load_from_file(&self.initialize.required_files).await?;

        Ok(transport::ApptainerJobInit {
            sif_bytes,
            batch_name,
            build_files,
            container_bind_paths: self.initialize.required_mounts.clone(),
        })
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
    sif: PathBuf,
    #[serde(default)]
    required_files: Vec<File>,
    /// paths in the folder that need to have mount points to
    /// the host file system
    #[serde(default)]
    required_mounts: Vec<PathBuf>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Constructor)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "python", pyo3::pyclass)]
pub struct Job {
    name: String,
    #[serde(default)]
    required_files: Vec<File>,
}
