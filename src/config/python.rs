use super::LoadJobsError;
use super::NormalizePaths;
use super::ReadBytesError;
use derive_more::Constructor;

#[cfg(feature = "cli")]
use crate::transport;

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[cfg(feature = "cli")]
use super::common::load_from_file;
use super::common::File;

#[derive(Debug, Clone, Deserialize, Serialize, Constructor)]
#[serde(deny_unknown_fields)]
pub struct Description {
    initialize: Initialize,
    jobs: Vec<Job>,
}

#[cfg(feature = "cli")]
impl Description {
    pub(crate) fn len_jobs(&self) -> usize {
        self.jobs.len()
    }

    pub(crate) async fn load_jobs(&self) -> Result<Vec<transport::PythonJob>, LoadJobsError> {
        let mut out = Vec::with_capacity(self.jobs.len());

        for job in &self.jobs {
            let bytes = tokio::fs::read(&job.python_job_file).await.map_err(|e| {
                LoadJobsError::from(ReadBytesError::new(e, job.python_job_file.clone()))
            })?;

            let job_files = load_from_file(&job.required_files).await?;

            let job = transport::PythonJob {
                python_file: bytes,
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
    ) -> Result<transport::PythonJobInit, LoadJobsError> {
        let bytes = tokio::fs::read(&self.initialize.python_build_file_path)
            .await
            .map_err(|e| ReadBytesError::new(e, self.initialize.python_build_file_path.clone()))?;

        let additional_build_files = load_from_file(&self.initialize.required_files).await?;

        debug!(
            "number of initial files included: {}",
            additional_build_files.len()
        );

        Ok(transport::PythonJobInit {
            batch_name,
            python_setup_file: bytes,
            additional_build_files,
        })
    }
}

impl NormalizePaths for Description {
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

#[derive(Debug, Clone, Deserialize, Serialize, Constructor)]
#[serde(deny_unknown_fields)]
pub struct Initialize {
    #[serde(rename = "build_file")]
    pub python_build_file_path: PathBuf,
    #[serde(default)]
    required_files: Vec<File>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Constructor)]
#[serde(deny_unknown_fields)]
pub struct Job {
    name: String,
    #[serde(rename = "file")]
    python_job_file: PathBuf,
    #[serde(default)]
    required_files: Vec<File>,
}
