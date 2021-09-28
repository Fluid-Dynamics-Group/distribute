use crate::error::{self, ConfigErrorReason, ConfigurationError};
use crate::{server, transport};
use derive_more::Display;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use std::path::PathBuf;

use super::common::{load_from_file, File};

#[derive(Debug, Clone, Deserialize)]
pub struct Description {
    initialize: Initialize,
    jobs: Vec<Job>,
}

impl Description {
    pub(crate) async fn load_jobs(
        &self,
        batch_name: &str,
    ) -> Result<Vec<transport::PythonJob>, error::LoadJobsError> {
        let mut out = Vec::with_capacity(self.jobs.len());

        for job in &self.jobs {
            let bytes = tokio::fs::read(&job.python_job_file).await.map_err(|e| {
                error::LoadJobsError::from(error::ReadBytesError::new(
                    e,
                    job.python_job_file.clone(),
                ))
            })?;

            let job_files = load_from_file(&job.required_files).await?;

            let job = transport::PythonJob {
                python_file: bytes,
                job_name: job.name.clone(),
                batch_name: batch_name.to_string(),
                job_files,
            };
            out.push(job)
        }

        Ok(out)
    }

    pub(crate) async fn load_build(
        &self,
        batch_name: String,
    ) -> Result<transport::PythonJobInit, error::LoadJobsError> {
        let bytes = tokio::fs::read(&self.initialize.python_build_file_path)
            .await
            .map_err(|e| {
                error::ReadBytesError::new(e, self.initialize.python_build_file_path.clone())
            })?;

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

#[derive(Debug, Clone, Deserialize)]
pub struct Initialize {
    #[serde(rename = "build_file")]
    pub python_build_file_path: PathBuf,
    #[serde(default)]
    required_files: Vec<File>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Job {
    name: String,
    #[serde(rename = "file")]
    python_job_file: PathBuf,
    #[serde(default)]
    required_files: Vec<File>,
}
