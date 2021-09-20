use crate::error::{self, ConfigErrorReason, ConfigurationError};
use crate::{server, transport};
use derive_more::Display;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize)]
pub struct Nodes {
    pub nodes: Vec<Node>,
}

#[derive(Debug, Clone, Deserialize, Display)]
#[display(fmt = "ip address: {}", ip)]
pub struct Node {
    pub(crate) ip: std::net::IpAddr,
    #[serde(default = "default_client_port")]
    pub(crate) port: u16,
    pub(crate) capabilities: server::Requirements<server::NodeProvidedCaps>,
}

fn default_client_port() -> u16 {
    crate::cli::CLIENT_PORT
}

impl Node {
    pub(crate) fn addr(&self) -> std::net::SocketAddr {
        std::net::SocketAddr::from((self.ip, self.port))
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Jobs {
    pub init: BuildJob,
    pub jobs: Vec<Job>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BuildJob {
    #[serde(rename = "build_file")]
    pub python_build_file_path: PathBuf,
    #[serde(default)]
    required_files: Vec<PathBuf>,
    pub(crate) batch_name: String,
    pub(crate) namespace: String,
    pub(crate) matrix: Option<matrix_notify::UserId>,
    pub(crate) capabilities: server::Requirements<server::JobRequiredCaps>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Job {
    name: String,
    #[serde(rename = "file")]
    job_file: PathBuf,
}

impl Jobs {
    pub(crate) async fn load_jobs(&self) -> Result<Vec<transport::Job>, error::LoadJobsError> {
        let mut out = Vec::with_capacity(self.jobs.len());

        for job in &self.jobs {
            let bytes = tokio::fs::read(&job.job_file).await.map_err(|e| {
                error::LoadJobsError::from(error::ReadBytesError::new(e, job.job_file.clone()))
            })?;
            let job = transport::Job {
                python_file: bytes,
                job_name: job.name.clone(),
            };
            out.push(job)
        }

        Ok(out)
    }

    pub(crate) async fn load_build(&self) -> Result<transport::JobInit, error::LoadJobsError> {
        let bytes = tokio::fs::read(&self.init.python_build_file_path)
            .await
            .map_err(|e| error::ReadBytesError::new(e, self.init.python_build_file_path.clone()))?;

        let mut additional_build_files = vec![];

        for additional_file in &self.init.required_files {
            let additional_bytes = tokio::fs::read(&additional_file)
                .await
                .map_err(|e| error::ReadBytesError::new(e, additional_file.clone()))?;

            let file_name = additional_file
                .file_name()
                .ok_or(error::MissingFileNameError::from(additional_file.clone()))?
                .to_string_lossy()
                .to_string();

            additional_build_files.push(transport::BuildFile {
                file_name,
                file_bytes: additional_bytes,
            });
        }

        debug!(
            "number of initial files included: {}",
            additional_build_files.len()
        );

        Ok(transport::JobInit {
            batch_name: self.init.batch_name.clone(),
            python_setup_file: bytes,
            additional_build_files,
        })
    }
}

pub fn load_config<T: DeserializeOwned>(path: &str) -> Result<T, ConfigurationError> {
    let file =
        std::fs::File::open(path).map_err(|e| (path.to_string(), ConfigErrorReason::from(e)))?;

    let config = serde_yaml::from_reader(file)
        .map_err(|e| (path.to_string(), ConfigErrorReason::from(e)))?;

    Ok(config)
}

#[test]
fn serialize_nodes() {
    let bytes = include_str!("../static/example-nodes.yaml");
    let _out: Nodes = serde_yaml::from_str(bytes).unwrap();
}

#[test]
fn serialize_jobs() {
    let bytes = include_str!("../static/example-jobs.yaml");
    let _out: Jobs = serde_yaml::from_str(bytes).unwrap();
}
