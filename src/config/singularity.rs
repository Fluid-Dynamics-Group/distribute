use crate::error::{self, ConfigErrorReason, ConfigurationError};
use crate::{server, transport};
use derive_more::{Constructor, Display};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::common::{load_from_file, File};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Description {
    pub initialize: Initialize,
    pub jobs: Vec<Job>,
}

impl Description {
    pub(crate) async fn load_jobs(
        &self,
    ) -> Result<Vec<transport::SingularityJob>, error::LoadJobsError> {
        let mut out = Vec::with_capacity(self.jobs.len());

        for job in &self.jobs {
            let job_files = load_from_file(&job.required_files).await?;

            let job = transport::SingularityJob {
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
    ) -> Result<transport::SingularityJobInit, error::LoadJobsError> {
        let sif_bytes = tokio::fs::read(&self.initialize.sif)
            .await
            .map_err(|e| error::ReadBytesError::new(e, self.initialize.sif.clone()))?;

        let build_files = load_from_file(&self.initialize.required_files).await?;

        Ok(transport::SingularityJobInit {
            sif_bytes,
            batch_name,
            build_files,
        })
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, Constructor)]
pub struct Initialize {
    sif: PathBuf,
    #[serde(default)]
    required_files: Vec<File>,
    #[serde(default)]
    /// if specified, create a mutable filesystem of this many megabytes.
    ///
    /// This parameter is not requried if your image is immutable. If you require
    /// more than a few gigabytes of storage, it is recommended to refactor the code
    /// to utilize a filesystem mount (to the host machin) directly to
    filesystem_size: Option<u32>,
    #[serde(default)]
    required_mounts: Vec<PathBuf>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Constructor)]
pub struct Job {
    name: String,
    #[serde(default)]
    required_files: Vec<File>,
}
