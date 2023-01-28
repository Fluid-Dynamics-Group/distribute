pub mod apptainer;
pub mod common;
pub mod python;
pub mod requirements;

#[cfg(feature = "cli")]
mod hashing;

#[cfg(feature = "cli")]
use crate::transport;

#[cfg(feature = "cli")]
use crate::client::execute::FileMetadata;

use derive_more::{Constructor, Display, From, Unwrap};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::path::PathBuf;

use getset::Getters;

macro_rules! const_port {
    ($NUMERIC:ident, $STR:ident, $value:expr) => {
        pub const $NUMERIC: u16 = $value;
        pub const $STR: &'static str = stringify!($value);
    };
}

//
// ports for different communication channels
//
const_port!(SERVER_PORT, SERVER_PORT_STR, 8952);
const_port!(CLIENT_PORT, CLIENT_PORT_STR, 8953);
const_port!(CLIENT_KEEPALIVE_PORT, CLIENT_KEEPALIVE_PORT_STR, 8954);
const_port!(CLIENT_CANCEL_PORT, CLIENT_CANCEL_PORT_STR, 8955);

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
    #[display(fmt = "missing file: {}", _0)]
    MissingFile(MissingFileNameError),
    #[display(fmt = "General Io error when opening config file: {}", _0)]
    IoError(std::io::Error),
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
#[display(
    fmt = "Failed to canonicalize path {} - error: {}",
    "path.display()",
    err
)]
/// happens when calling .canonicalize() on a path
pub struct CanonicalizeError {
    path: PathBuf,
    err: std::io::Error,
}

#[derive(Debug, Display, From, thiserror::Error, Constructor)]
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
    #[serde(default = "default_cancel_port")]
    pub(crate) cancel_port: u16,
    pub(crate) capabilities: requirements::Requirements<requirements::NodeProvidedCaps>,
}

fn default_client_port() -> u16 {
    CLIENT_PORT
}

fn default_keepalive_port() -> u16 {
    CLIENT_KEEPALIVE_PORT
}

fn default_cancel_port() -> u16 {
    CLIENT_CANCEL_PORT
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

    /// create the full address to the node's port at which they cancel the executing jobs
    pub(crate) fn cancel_addr(&self) -> std::net::SocketAddr {
        std::net::SocketAddr::from((self.ip, self.cancel_port))
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, From)]
#[serde(untagged)]
#[serde(deny_unknown_fields)]
pub enum Jobs<FILE> {
    Python(PythonConfig<FILE>),
    Apptainer(ApptainerConfig<FILE>),
}

#[derive(
    Debug, Clone, Deserialize, Serialize, Constructor, getset::Getters, getset::MutGetters,
)]
#[serde(deny_unknown_fields)]
pub struct ApptainerConfig<FILE> {
    #[getset(get = "pub(crate)")]
    meta: Meta,
    #[serde(rename = "apptainer")]
    #[getset(get = "pub(crate)", get_mut = "pub(crate)")]
    description: apptainer::Description<FILE>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Constructor, Getters)]
#[serde(deny_unknown_fields)]
pub struct PythonConfig<FILE> {
    #[getset(get = "pub(crate)")]
    meta: Meta,
    #[serde(rename = "python")]
    #[getset(get = "pub(crate)")]
    description: python::Description<FILE>,
}

#[derive(Debug, Clone, Deserialize, Serialize, From)]
pub enum Init {
    Python(python::Initialize<common::HashedFile>),
    Apptainer(apptainer::Initialize<common::HashedFile>),
}

impl From<&Jobs<common::HashedFile>> for Init {
    fn from(config: &Jobs<common::HashedFile>) -> Init {
        match &config {
            Jobs::Apptainer(app) => Init::Apptainer(app.description.initialize.clone()),
            Jobs::Python(py) => Init::Python(py.description.initialize.clone()),
        }
    }
}

#[cfg(feature="cli")]
impl Init {
    pub(crate) fn sendable_files(&self, is_user: bool) -> Vec<FileMetadata> {
        let mut out = Vec::new();

        match &self {
            Self::Python(py) => py.sendable_files(is_user, &mut out),
            Self::Apptainer(app) => app.sendable_files(is_user, &mut out),
        }

        out
    }

    pub(crate) fn delete_files(self) -> Result<(), std::io::Error> {
        match self {
            Self::Python(py) => {
                py.python_build_file_path.delete_at_hashed_path()?;
                common::delete_hashed_files(py.required_files)
            }
            Self::Apptainer(app) => {
                app.sif.delete_at_hashed_path()?;
                common::delete_hashed_files(app.required_files)
            }
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, From)]
pub enum Job {
    Python(python::Job<common::HashedFile>),
    Apptainer(apptainer::Job<common::HashedFile>),
}

#[cfg(feature="cli")]
impl Job {
    pub(crate) fn sendable_files(&self, is_user: bool) -> Vec<FileMetadata> {
        let mut out = Vec::new();

        match &self {
            Self::Python(py) => py.sendable_files(is_user, &mut out),
            Self::Apptainer(app) => app.sendable_files(is_user, &mut out),
        }

        out
    }

    pub(crate) fn name(&self) -> &str {
        match &self {
            Self::Python(py) => &py.name(),
            Self::Apptainer(app) => &app.name(),
        }
    }

    pub(crate) fn delete_files(self) -> Result<(), std::io::Error> {
        match self {
            Self::Python(py) => {
                py.python_job_file().delete_at_hashed_path()?;
                common::delete_hashed_files(py.required_files)
            }
            Self::Apptainer(app) => {
                common::delete_hashed_files(app.required_files)
            }
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, Constructor, Getters)]
#[serde(deny_unknown_fields)]
pub struct Meta {
    #[getset(get = "pub(crate)")]
    pub batch_name: String,
    #[getset(get = "pub(crate)")]
    pub namespace: String,
    pub matrix: Option<matrix_notify::OwnedUserId>,
    pub capabilities: requirements::Requirements<requirements::JobRequiredCaps>,
}

#[cfg(feature = "cli")]
impl Jobs<common::File> {
    pub fn len_jobs(&self) -> usize {
        match &self {
            Self::Python(pyconfig) => pyconfig.description.len_jobs(),
            Self::Apptainer(apptainer_config) => apptainer_config.description.len_jobs(),
        }
    }

    /// ensure that all paths exist as we expect them to
    pub(crate) fn verify_config(&self) -> Result<(), ConfigErrorReason> {
        match &self {
            Self::Python(py) => py.description.verify_config()?,
            Self::Apptainer(app) => app.description.verify_config()?,
        };

        Ok(())
    }

    pub fn hashed(&self) -> Result<Jobs<common::HashedFile>, MissingFileNameError> {
        match &self {
            Self::Python(pyconfig) => {
                let description = pyconfig.description.hashed()?;

                Ok(Jobs::from(PythonConfig {
                    meta: pyconfig.meta.clone(),
                    description,
                }))
            }
            Self::Apptainer(apptainer_config) => {
                let description = apptainer_config.description.hashed()?;

                Ok(Jobs::from(ApptainerConfig {
                    meta: apptainer_config.meta.clone(),
                    description,
                }))
            }
        }
    }
}

#[cfg(feature="cli")]
impl Jobs<common::HashedFile> {
    pub(crate) fn sendable_files(&self, is_user: bool) -> Vec<FileMetadata> {
        match &self {
            Jobs::Python(py) => py.description.sendable_files(is_user),
            Jobs::Apptainer(app) => app.description.sendable_files(is_user),
        }
    }
}

impl<FILE> Jobs<FILE> {
    pub(crate) fn capabilities(
        &self,
    ) -> &requirements::Requirements<requirements::JobRequiredCaps> {
        match &self {
            Self::Python(py) => &py.meta.capabilities,
            Self::Apptainer(app) => &app.meta.capabilities,
        }
    }

    pub fn batch_name(&self) -> String {
        match self {
            Self::Python(py) => py.meta.batch_name.clone(),
            Self::Apptainer(app) => app.meta.batch_name.clone(),
        }
    }

    pub fn matrix_user(&self) -> Option<matrix_notify::OwnedUserId> {
        match self {
            Self::Python(py) => py.meta.matrix.clone(),
            Self::Apptainer(app) => app.meta.matrix.clone(),
        }
    }

    pub fn namespace(&self) -> String {
        match self {
            Self::Python(py) => py.meta.namespace.clone(),
            Self::Apptainer(app) => app.meta.namespace.clone(),
        }
    }

    pub fn job_names(&self) -> Vec<&str> {
        match self {
            Self::Python(py) => py
                .description
                .jobs
                .iter()
                .map(|job| job.name().as_str())
                .collect(),
            Self::Apptainer(apt) => apt
                .description
                .jobs
                .iter()
                .map(|job| job.name().as_str())
                .collect(),
        }
    }
}

impl<FILE> Jobs<FILE>
where
    FILE: Serialize,
{
    /// write the config file to a provided `Write`r
    pub fn to_writer<W: std::io::Write>(&self, writer: W) -> Result<(), serde_yaml::Error> {
        serde_yaml::to_writer(writer, &self)?;

        Ok(())
    }
}

impl ApptainerConfig<common::File> {
    /// write the config file to a provided `Write`r
    pub fn to_writer<W: std::io::Write>(&self, writer: W) -> Result<(), serde_yaml::Error> {
        serde_yaml::to_writer(writer, &self)?;

        Ok(())
    }
}

pub trait NormalizePaths {
    fn normalize_paths(&mut self, base: PathBuf);
}

impl<FILE> NormalizePaths for Jobs<FILE>
where
    FILE: NormalizePaths,
{
    fn normalize_paths(&mut self, base: PathBuf) {
        match self {
            Self::Python(py) => py.description.normalize_paths(base),
            Self::Apptainer(app) => app.description.normalize_paths(base),
        }
    }
}

impl NormalizePaths for Nodes {
    fn normalize_paths(&mut self, _base: PathBuf) {
        // we dont actually need to normalize things for nodes configuration
    }
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

    #[test]
    fn strings_match() {
        assert_eq!(SERVER_PORT.to_string(), SERVER_PORT_STR.to_string());
    }
}
