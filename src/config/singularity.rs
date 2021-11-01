use crate::error::{self};
use crate::transport;
use derive_more::Constructor;

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
            container_bind_paths: self.initialize.required_mounts.clone(),
        })
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, Constructor)]
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
pub struct Job {
    name: String,
    #[serde(default)]
    required_files: Vec<File>,
}
