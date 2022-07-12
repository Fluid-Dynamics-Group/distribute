pub mod apptainer;
pub mod common;
pub mod python;
pub mod requirements;

#[cfg(feature = "cli")]
use crate::transport;

use derive_more::{Constructor, Display, From, Unwrap};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::path::PathBuf;

#[allow(dead_code)]
pub const SERVER_PORT: u16 = 8952;
pub const SERVER_PORT_STR: &'static str = "8952";

pub const CLIENT_PORT: u16 = 8953;
pub const CLIENT_PORT_STR: &'static str = "8953";

pub const CLIENT_KEEPALIVE_PORT: u16 = 8954;
pub const CLIENT_KEEPALIVE_PORT_STR: &'static str = "8954";

#[derive(Debug, Display, thiserror::Error, From)]
#[display(
    fmt = "configuration file: `{}` reason: `{}`",
    "configuration_file",
    "error"
)]
pub struct ConfigurationError {
    configuration_file: String,
    error: ConfigErrorReason,
}

#[derive(Debug, Display, From)]
pub enum ConfigErrorReason {
    #[display(fmt = "deserialization error: {}", _0)]
    Deserialization(serde_yaml::Error),
    MissingFile(std::io::Error),
}

#[derive(Debug, From, thiserror::Error)]
pub enum LoadJobsError {
    #[error("{0}")]
    ReadBytes(ReadBytesError),
    #[error("{0}")]
    MissingFileName(MissingFileNameError),
    #[error("{0}")]
    Canonicalize(CanonicalizeError),
}

#[derive(Debug, From, thiserror::Error, Constructor, Display)]
#[display(fmt = "Failed to canonicalize path {} - error: {}", "path.display()", err)]
/// happens when calling .canonicalize() on a path
pub struct CanonicalizeError {
    path: PathBuf,
    err: std::io::Error,
}

#[derive(Debug, Display, From, thiserror::Error)]
#[display(fmt = "Error loading configuration for jobs {:?} ", path)]
/// happens when a file path does not contain a filename
pub struct MissingFileNameError {
    path: PathBuf,
}

#[derive(Debug, Display, From, thiserror::Error, Constructor)]
#[display(
    fmt = "Error loading configuration for jobs (`{}`): {:?} ",
    error,
    path
)]
/// error that happens when loading the bytes of a job file from path
pub struct ReadBytesError {
    error: std::io::Error,
    path: std::path::PathBuf,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
/// main entry point for server configuration file
pub struct Nodes {
    pub nodes: Vec<Node>,
}

#[derive(Debug, Clone, Deserialize, Display)]
#[serde(deny_unknown_fields)]
#[display(fmt = "ip address: {}", ip)]
pub struct Node {
    pub(crate) ip: std::net::IpAddr,
    #[serde(rename = "name")]
    pub(crate) node_name: String,
    #[serde(default = "default_client_port")]
    pub(crate) transport_port: u16,
    #[serde(default = "default_keepalive_port")]
    pub(crate) keepalive_port: u16,
    pub(crate) capabilities: requirements::Requirements<requirements::NodeProvidedCaps>,
}

fn default_client_port() -> u16 {
    CLIENT_PORT
}

fn default_keepalive_port() -> u16 {
    CLIENT_KEEPALIVE_PORT
}

impl Node {
    /// create the full address to the node's port at which they receive jobs
    pub(crate) fn transport_addr(&self) -> std::net::SocketAddr {
        std::net::SocketAddr::from((self.ip, self.transport_port))
    }

    /// create the full address to the node's port at which they check for keepalive connections
    pub(crate) fn keepalive_addr(&self) -> std::net::SocketAddr {
        std::net::SocketAddr::from((self.ip, self.keepalive_port))
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
#[serde(deny_unknown_fields)]
pub enum Jobs {
    Python {
        meta: Meta,
        python: python::Description,
    },
    Apptainer {
        meta: Meta,
        apptainer: apptainer::Description,
    },
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Meta {
    pub batch_name: String,
    pub namespace: String,
    pub matrix: Option<matrix_notify::UserId>,
    pub capabilities: requirements::Requirements<requirements::JobRequiredCaps>,
}

#[cfg(feature = "cli")]
impl Jobs {
    pub fn len_jobs(&self) -> usize {
        match &self {
            Self::Python { meta: _, python } => python.len_jobs(),
            Self::Apptainer { meta: _, apptainer } => apptainer.len_jobs(),
        }
    }
    pub async fn load_jobs(&self) -> Result<JobOpts, LoadJobsError> {
        match &self {
            Self::Python { meta: _, python } => {
                let py_jobs = python.load_jobs().await?;
                Ok(py_jobs.into())
            }
            Self::Apptainer { meta: _, apptainer } => {
                let sin_jobs = apptainer.load_jobs().await?;
                Ok(sin_jobs.into())
            }
        }
    }

    pub async fn load_build(&self) -> Result<transport::BuildOpts, LoadJobsError> {
        match &self {
            Self::Python { meta, python } => {
                let py_build = python.load_build(meta.batch_name.clone()).await?;
                Ok(py_build.into())
            }
            Self::Apptainer { meta, apptainer } => {
                let sin_build = apptainer.load_build(meta.batch_name.clone()).await?;
                Ok(sin_build.into())
            }
        }
    }

    pub(crate) fn capabilities(
        &self,
    ) -> &requirements::Requirements<requirements::JobRequiredCaps> {
        match &self {
            Self::Python { meta, .. } => &meta.capabilities,
            Self::Apptainer { meta, .. } => &meta.capabilities,
        }
    }

    pub fn batch_name(&self) -> String {
        match self {
            Self::Python { meta, .. } => meta.batch_name.clone(),
            Self::Apptainer { meta, .. } => meta.batch_name.clone(),
        }
    }

    pub fn matrix_user(&self) -> Option<matrix_notify::UserId> {
        match self {
            Self::Python { meta, .. } => meta.matrix.clone(),
            Self::Apptainer { meta, .. } => meta.matrix.clone(),
        }
    }

    pub fn namespace(&self) -> String {
        match self {
            Self::Python { meta, .. } => meta.namespace.clone(),
            Self::Apptainer { meta, .. } => meta.namespace.clone(),
        }
    }
}

impl Jobs {
    /// write the config file to a provided `Write`r
    pub fn to_writer<W: std::io::Write>(&self, writer: W) -> Result<(), serde_yaml::Error> {
        serde_yaml::to_writer(writer, &self)?;

        Ok(())
    }
}

pub trait NormalizePaths {
    fn normalize_paths(&mut self, base: PathBuf);
}

impl NormalizePaths for Jobs {
    fn normalize_paths(&mut self, base: PathBuf) {
        match self {
            Self::Python {
                meta: _meta,
                python,
            } => python.normalize_paths(base),
            Self::Apptainer {
                meta: _meta,
                apptainer,
            } => apptainer.normalize_paths(base),
        }
    }
}

impl NormalizePaths for Nodes {
    fn normalize_paths(&mut self, _base: PathBuf) {
        // we dont actually need to normalize things for nodes configuration
    }
}

#[derive(derive_more::From, Serialize, Deserialize, Clone, Debug, Unwrap)]
#[serde(deny_unknown_fields)]
#[cfg(feature = "cli")]
pub enum JobOpts {
    Python(Vec<transport::PythonJob>),
    Apptainer(Vec<transport::ApptainerJob>),
}

pub fn load_config<T: DeserializeOwned + NormalizePaths>(
    path: &Path,
) -> Result<T, ConfigurationError> {
    let file = std::fs::File::open(path)
        .map_err(|e| (path.display().to_string(), ConfigErrorReason::from(e)))?;

    let mut config: T = serde_yaml::from_reader(file)
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
fn serialize_jobs_apptainer() {
    let bytes = include_str!("../../static/example-jobs-apptainer.yaml");
    let _out: Jobs = serde_yaml::from_str(bytes).unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct PythonConfiguration {
        meta: Meta,
        python: python::Description,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct ApptainerConfiguration {
        meta: Meta,
        apptainer: apptainer::Description,
    }

    #[test]
    fn serialize_python() {
        let bytes = include_str!("../../static/example-jobs-python.yaml");
        let _out: PythonConfiguration = serde_yaml::from_str(bytes).unwrap();
    }

    #[test]
    fn serialize_apptainer() {
        let bytes = include_str!("../../static/example-jobs-apptainer.yaml");
        let _out: ApptainerConfiguration = serde_yaml::from_str(bytes).unwrap();
    }
}
