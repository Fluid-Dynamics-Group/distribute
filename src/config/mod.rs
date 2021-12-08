pub mod common;
pub mod python;
pub mod singularity;

use crate::error::{self, ConfigErrorReason, ConfigurationError};
use crate::{server, transport};
use derive_more::{Display, Unwrap};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::path::Path;

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

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
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

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Meta {
    pub batch_name: String,
    pub namespace: String,
    pub matrix: Option<matrix_notify::UserId>,
    pub capabilities: server::Requirements<server::JobRequiredCaps>,
}

impl Jobs {
    pub async fn load_jobs(&self) -> Result<JobOpts, error::LoadJobsError> {
        match &self {
            Self::Python { meta: _, python } => {
                let py_jobs = python.load_jobs().await?;
                Ok(py_jobs.into())
            }
            Self::Singularity {
                meta: _,
                singularity,
            } => {
                let sin_jobs = singularity.load_jobs().await?;
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

    pub(crate) fn capabilities(&self) -> &server::Requirements<server::JobRequiredCaps> {
        match &self {
            Self::Python { meta, .. } => &meta.capabilities,
            Self::Singularity { meta, .. } => &meta.capabilities,
        }
    }

    pub fn batch_name(&self) -> String {
        match self {
            Self::Python { meta, .. } => meta.batch_name.clone(),
            Self::Singularity { meta, .. } => meta.batch_name.clone(),
        }
    }

    pub fn matrix_user(&self) -> Option<matrix_notify::UserId> {
        match self {
            Self::Python { meta, .. } => meta.matrix.clone(),
            Self::Singularity { meta, .. } => meta.matrix.clone(),
        }
    }

    pub fn namespace(&self) -> String {
        match self {
            Self::Python { meta, .. } => meta.namespace.clone(),
            Self::Singularity { meta, .. } => meta.namespace.clone(),
        }
    }
}

pub trait NormalizePaths {
    fn normalize_paths(&mut self, base: PathBuf);
}

impl NormalizePaths for Jobs {
    fn normalize_paths(&mut self, base: PathBuf) {
        match self {
            Self::Python {meta: _meta, python } => python.normalize_paths(base),
            Self::Singularity {meta: _meta, singularity } => singularity.normalize_paths(base)
        }
    }
}

impl NormalizePaths for Nodes {
    fn normalize_paths(&mut self, _base: PathBuf) {
        // we dont actually need to normalize things for nodes configuration
    }
}

#[derive(derive_more::From, Serialize, Deserialize, Clone, Debug, Unwrap)]
pub enum JobOpts {
    Python(Vec<transport::PythonJob>),
    Singularity(Vec<transport::SingularityJob>),
}

#[derive(derive_more::From, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum BuildOpts {
    Python(transport::PythonJobInit),
    Singularity(transport::SingularityJobInit),
}

impl BuildOpts {
    pub(crate) fn batch_name(&self) -> &str {
        match &self {
            Self::Singularity(s) => &s.batch_name,
            Self::Python(p) => &p.batch_name,
        }
    }
}

pub fn load_config<T: DeserializeOwned + NormalizePaths>(path: &Path) -> Result<T, ConfigurationError> {
    let file =
        std::fs::File::open(path).map_err(|e| (path.display().to_string(), ConfigErrorReason::from(e)))?;

    let mut config : T= serde_yaml::from_reader(file)
        .map_err(|e| (path.display().to_string(), ConfigErrorReason::from(e)))?;

    config.normalize_paths(path.parent().unwrap().to_owned());

    Ok(config)
}

#[test]
fn serialize_nodes() {
    let bytes = include_str!("../../static/example-nodes.yaml");
    let _out: Nodes = serde_yaml::from_str(bytes).unwrap();
}

#[test]
fn serialize_jobs_python() {
    let bytes = include_str!("../../static/example-jobs-python.yaml");
    let _out: Jobs = serde_yaml::from_str(bytes).unwrap();
}

#[test]
fn serialize_jobs_singularity() {
    let bytes = include_str!("../../static/example-jobs-singularity.yaml");
    let _out: Jobs = serde_yaml::from_str(bytes).unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Deserialize)]
    struct PythonConfiguration {
        meta: Meta,
        python: python::Description,
    }

    #[derive(Deserialize)]
    struct SingularityConfiguration {
        meta: Meta,
        singularity: singularity::Description,
    }

    #[test]
    fn serialize_python() {
        let bytes = include_str!("../../static/example-jobs-python.yaml");
        let _out: PythonConfiguration = serde_yaml::from_str(bytes).unwrap();
    }

    #[test]
    fn serialize_singularity() {
        let bytes = include_str!("../../static/example-jobs-singularity.yaml");
        let _out: SingularityConfiguration = serde_yaml::from_str(bytes).unwrap();
    }
}
