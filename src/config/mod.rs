mod common;
mod python;
mod singularity;

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
pub enum Jobs {
    Python {
        meta: Meta,
        python: python::Description,
    },
    Singularity {
        meta: Meta,
        singularity: singularity::Description,
    },
}

#[derive(Debug, Clone, Deserialize)]
struct Meta {
    pub(crate) batch_name: String,
    pub(crate) namespace: String,
    pub(crate) matrix: Option<matrix_notify::UserId>,
    pub(crate) capabilities: server::Requirements<server::JobRequiredCaps>,
}

impl Jobs {
    pub async fn load_jobs(&self) -> Result<JobOpts, error::LoadJobsError> {
        match &self {
            Self::Python { meta, python } => {
                let py_jobs = python.load_jobs().await?;
                Ok(py_jobs.into())
            }
            Self::Singularity { meta, singularity } => {
                let sin_jobs = singularity.load_jobs(meta.batch_name.clone()).await?;
                Ok(sin_jobs.into())
            }
        }
    }

    pub async fn load_build(&self) -> Result<BuildOpts, error::LoadJobsError> {
        match &self {
            Self::Python { meta, python } => {
                let py_build = python.load_build(meta.batch_name.clone()).await?;
                Ok(py_build.into())
            }
            Self::Singularity { meta, singularity } => {
                let sin_build = singularity.load_build(meta.batch_name.clone()).await?;
                Ok(sin_build.into())
            }
        }
    }
}

#[derive(derive_more::From)]
pub enum JobOpts {
    Python(Vec<transport::PythonJob>),
    Singularity(Vec<transport::SingularityJob>),
}

#[derive(derive_more::From)]
pub enum BuildOpts {
    Python(transport::PythonJobInit),
    Singularity(transport::SingularityJobInit),
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
