/// apptainer configuration types
pub mod apptainer;
/// helper types common between python and apptainer
pub mod common;
/// python configuration types
pub mod python;
/// helper types for job or compute node capabilities
pub mod requirements;

#[cfg(feature = "cli")]
mod hashing;

#[cfg(feature = "cli")]
use crate::client::execute::FileMetadata;

use derive_more::{Constructor, Display, From};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;

use getset::Getters;
use getset::Setters;

macro_rules! const_port {
    ($NUMERIC:ident, $STR:ident, $value:expr, $docstring:expr) => {
        #[doc=$docstring]
        pub const $NUMERIC: u16 = $value;
        #[doc=$docstring]
        pub const $STR: &'static str = stringify!($value);
    };
}

//
// ports for different communication channels
//
const_port!(
    SERVER_PORT,
    SERVER_PORT_STR,
    8952,
    "default port that the server listens on for user connections"
);
const_port!(
    CLIENT_PORT,
    CLIENT_PORT_STR,
    8953,
    "default port that the client listens on for server compute connections"
);
const_port!(
    CLIENT_KEEPALIVE_PORT,
    CLIENT_KEEPALIVE_PORT_STR,
    8954,
    "default port that the client listens on for server keepalive checks"
);
const_port!(
    CLIENT_CANCEL_PORT,
    CLIENT_CANCEL_PORT_STR,
    8955,
    "default port that the client listens on for server cancellation notices"
);

#[derive(Debug, Display, thiserror::Error, Constructor)]
#[display(
    fmt = "configuration file: `{}` reason: `{}`",
    "configuration_file",
    "error"
)]
/// helper to hold config error and the path at which the file was located
pub struct ConfigurationError {
    configuration_file: String,
    error: ConfigErrorReason,
}

#[derive(Debug, Display, From)]
/// failure / incorrect data in config file
pub enum ConfigErrorReason {
    #[display(fmt = "{}", _0)]
    /// config file could not be deserialized
    Deserialization(DeserError),
    #[display(fmt = "missing file: {}", _0)]
    /// filename could not be parsed from the path, and no alias was provided
    MissingFile(MissingFilename),
    #[display(fmt = "General Io error when opening config file: {}", _0)]
    /// general io error
    IoError(std::io::Error),
}

#[derive(Debug, Display, Constructor)]
#[display(
    fmt = "general deserialization: {general}\npython deserialization err: {python_err}\nappatainer deserialization error: {apptainer_err}"
)]
/// failure to deserialize job configuration from file (python or apptainer)
pub struct DeserError {
    general: serde_yaml::Error,
    python_err: serde_yaml::Error,
    apptainer_err: serde_yaml::Error,
}

#[derive(Debug, From, thiserror::Error)]
/// error type for conditions in which jobs can not be loaded and read from disk
pub enum LoadJobsError {
    #[error("{0}")]
    /// failed to read bytes from a file
    ReadBytes(ReadBytesError),
    #[error("{0}")]
    /// filename could not be parsed from the path, and no alias was provided
    MissingFileName(MissingFilename),
    #[error("{0}")]
    /// failed to canonicalize paths
    Canonicalize(CanonicalizeError),
}

#[derive(Debug, Display, From, thiserror::Error, Constructor)]
#[display(
    fmt = "The filename for the path {} was missing, and no alias was supplied",
    "path.display()"
)]
/// a path was supplied to the configuration that does not have filename associated with the path
pub struct MissingFilename {
    path: PathBuf,
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
    /// list of nodes that the server can connect to
    pub nodes: Vec<Node>,
}

#[derive(Debug, Clone, Deserialize, Display)]
#[serde(deny_unknown_fields)]
#[display(fmt = "ip address: {}", ip)]
/// configuration information for a node that the server will connect to
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

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
/// special type that can be sent over TCP with bincode
///
/// this is because [`Jobs`] is required to serialize an untagged enum
/// which is impossible to do with binary parsing
pub enum TransportJobs<FILE> {
    /// python transportable configuration
    Python(PythonConfig<FILE>),
    /// apptainer transportable configuration
    Apptainer(ApptainerConfig<FILE>),
}

impl<F> From<Jobs<F>> for TransportJobs<F> {
    fn from(x: Jobs<F>) -> TransportJobs<F> {
        match x {
            Jobs::Python(py) => TransportJobs::Python(py),
            Jobs::Apptainer(app) => TransportJobs::Apptainer(app),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, From)]
#[cfg_attr(test, derive(derive_more::Unwrap))]
#[serde(untagged)]
#[serde(deny_unknown_fields)]
/// full configuration for either a python or apptainer job batch
pub enum Jobs<FILE> {
    /// full configuration for a python job batch
    Python(PythonConfig<FILE>),
    /// full configuration for an apptainer job batch
    Apptainer(ApptainerConfig<FILE>),
}

impl<F> From<TransportJobs<F>> for Jobs<F> {
    fn from(x: TransportJobs<F>) -> Jobs<F> {
        match x {
            TransportJobs::Python(py) => Jobs::Python(py),
            TransportJobs::Apptainer(app) => Jobs::Apptainer(app),
        }
    }
}

#[derive(
    Debug, Clone, Deserialize, Serialize, Constructor, getset::Getters, getset::MutGetters,
)]
#[serde(deny_unknown_fields)]
/// apptainer configuration file
pub struct ApptainerConfig<FILE> {
    /// Getters for configuration metadata
    #[getset(get = "pub(crate)", get_mut = "pub")]
    meta: Meta,
    #[serde(rename = "apptainer")]
    #[getset(get = "pub(crate)", get_mut = "pub")]
    /// description of the initialization and jobs for this config
    description: apptainer::Description<FILE>,
    #[getset(get = "pub(crate)")]
    slurm: Option<Slurm>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Constructor, Getters)]
#[serde(deny_unknown_fields)]
/// python configuration file
pub struct PythonConfig<FILE> {
    #[getset(get = "pub(crate)")]
    meta: Meta,
    #[serde(rename = "python")]
    #[getset(get = "pub(crate)")]
    description: python::Description<FILE>,
    slurm: Option<Slurm>,
}

#[derive(Debug, Clone, Deserialize, Serialize, From)]
/// initialization section for either an apptainer or python job batch
pub enum Init {
    /// initialization for python job batch
    Python(python::Initialize<common::HashedFile>),
    /// initialization for apptainer job batch
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

#[cfg(feature = "cli")]
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
/// Container for either a python job or an apptainer job
pub enum Job {
    /// initialization and job specification for a single python job
    Python(python::Job<common::HashedFile>),
    /// initialization and job specification for a single apptainer job
    Apptainer(apptainer::Job<common::HashedFile>),
}

#[cfg(feature = "cli")]
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
            Self::Apptainer(app) => common::delete_hashed_files(app.required_files),
        }
    }

    #[cfg(test)]
    pub(crate) fn placeholder_apptainer() -> Self {
        let job = apptainer::Job::new("some_job".into(), vec![], None);
        Self::from(job)
    }

    #[cfg(test)]
    pub(crate) fn placeholder_python(file: common::File) -> Self {
        let file = file.hashed().unwrap();
        let job = python::Job::new("python".into(), file, vec![], None);
        Self::from(job)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, Constructor, Getters, Setters)]
#[serde(deny_unknown_fields)]
/// A container for the `meta:` section of a distribute-jobs.yaml file
pub struct Meta {
    #[getset(get = "pub(crate)", set = "pub")]
    /// the batch name for the job set
    pub batch_name: String,
    #[getset(get = "pub(crate)")]
    /// the namespace for the job set
    pub namespace: String,
    /// the matrix user to message on completion
    pub matrix: Option<matrix_notify::OwnedUserId>,
    /// the capabilities required to execute the job
    pub capabilities: requirements::Requirements<requirements::JobRequiredCaps>,
}

#[cfg(feature = "cli")]
impl Jobs<common::File> {
    /// return the number of jobs in the job set
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

    /// hash all constitutent files of this job set in preparation to move them to the server
    pub fn hashed(&self) -> Result<Jobs<common::HashedFile>, MissingFilename> {
        match &self {
            Self::Python(pyconfig) => {
                let description = pyconfig.description.hashed(&pyconfig.meta)?;

                Ok(Jobs::from(PythonConfig {
                    meta: pyconfig.meta.clone(),
                    description,
                    slurm: pyconfig.slurm.clone(),
                }))
            }
            Self::Apptainer(apptainer_config) => {
                let description = apptainer_config
                    .description
                    .hashed(&apptainer_config.meta)?;

                Ok(Jobs::from(ApptainerConfig {
                    meta: apptainer_config.meta.clone(),
                    description,
                    slurm: apptainer_config.slurm.clone(),
                }))
            }
        }
    }
}

#[cfg(feature = "cli")]
impl Jobs<common::HashedFile> {
    pub(crate) fn sendable_files(&self, is_user: bool) -> Vec<FileMetadata> {
        match &self {
            Jobs::Python(py) => py.description.sendable_files(is_user),
            Jobs::Apptainer(app) => app.description.sendable_files(is_user),
        }
    }
}

impl<FILE> Jobs<FILE> {
    /// required capabilities to execute this job batch
    pub(crate) fn capabilities(
        &self,
    ) -> &requirements::Requirements<requirements::JobRequiredCaps> {
        match &self {
            Self::Python(py) => &py.meta.capabilities,
            Self::Apptainer(app) => &app.meta.capabilities,
        }
    }

    /// batch name of this job batch
    pub fn batch_name(&self) -> String {
        match self {
            Self::Python(py) => py.meta.batch_name.clone(),
            Self::Apptainer(app) => app.meta.batch_name.clone(),
        }
    }

    /// the matrix username to message on job completion
    pub fn matrix_user(&self) -> Option<matrix_notify::OwnedUserId> {
        match self {
            Self::Python(py) => py.meta.matrix.clone(),
            Self::Apptainer(app) => app.meta.matrix.clone(),
        }
    }

    /// namespace of the job batch
    pub fn namespace(&self) -> String {
        match self {
            Self::Python(py) => py.meta.namespace.clone(),
            Self::Apptainer(app) => app.meta.namespace.clone(),
        }
    }

    /// return all names of jobes in the job batch specified by the config file
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

/// normalize a path to a base path
pub trait NormalizePaths {
    /// normalize a path to a base path
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

/// read in a config file from disk
pub fn load_config<T: DeserializeOwned + NormalizePaths>(
    path: &Path,
    normalize: bool
) -> Result<T, ConfigurationError> {
    let file = std::fs::File::open(path).map_err(|e| {
        ConfigurationError::new(path.display().to_string(), ConfigErrorReason::from(e))
    })?;

    let config: Result<T, _> = serde_yaml::from_reader(file);

    let mut config = match config {
        Err(e) => {
            let file = std::fs::File::open(path).map_err(|e| {
                ConfigurationError::new(path.display().to_string(), ConfigErrorReason::from(e))
            })?;
            let python_err =
                serde_yaml::from_reader::<_, PythonConfig<common::File>>(file).unwrap_err();

            let file = std::fs::File::open(path).map_err(|e| {
                ConfigurationError::new(path.display().to_string(), ConfigErrorReason::from(e))
            })?;
            let apptainer_err =
                serde_yaml::from_reader::<_, ApptainerConfig<common::File>>(file).unwrap_err();

            let deser_error = DeserError {
                general: e,
                python_err,
                apptainer_err,
            };
            return Err(ConfigurationError::new(
                path.display().to_string(),
                ConfigErrorReason::from(deser_error),
            ));
        }
        Ok(config) => config,
    };

    if normalize {
        config.normalize_paths(path.parent().unwrap().to_owned());
    }

    Ok(config)
}

#[derive(Deserialize, Serialize, Clone, Debug, getset::Getters, Constructor)]
#[cfg_attr(feature = "python", pyo3::pyclass)]
/// fields that can be serialized to slurm parameters
pub struct Slurm {
    #[getset(get = "pub(crate)")]
    job_name: Option<String>,
    output: Option<String>,
    nodes: Option<usize>,
    #[getset(get = "pub(crate)")]
    ntasks: Option<usize>,
    cpus_per_task: Option<usize>,
    /// Ex: 10M
    mem_per_cpu: Option<String>,
    /// Ex: nomultithread
    hint: Option<String>,
    // TODO: could make this more robust
    time: Option<String>,
    /// Ex: cpu-core-0
    partition: Option<String>,
    account: Option<String>,
    mail_user: Option<String>,
    mail_type: Option<String>,
}

macro_rules! slurm_helper {
    ($overrides:ident, $self:ident; $($field:ident),*) => {
        Self {
            $(
                $field: $overrides.$field.as_ref().or($self.$field.as_ref()).cloned()
            ),*
        }
    };
    ($self:ident, $writer:ident; $($prefix:expr => $field:ident),*) => {
        $(
            if let Some(inner) = &$self.$field {
                let prefix = $prefix;
                writeln!(&mut $writer, "#SBATCH --{prefix}={inner}")?;
            }
        )*
    };
}

impl Slurm {
    /// Using `self` default values, override the values of `self` with `overrides` for each of the
    /// fields in `overrides` that are not none
    pub(crate) fn override_with(&self, overrides: &Self) -> Self {
        slurm_helper!(overrides, self;
            job_name,
            output,
            nodes,
            ntasks,
            cpus_per_task,
            mem_per_cpu,
            hint,
            time,
            partition,
            account,
            mail_user,
            mail_type
        )
    }

    pub(crate) fn set_default_job_name(&mut self, job_name: &str) {
        let job_name = job_name.to_string();
        self.job_name = self.job_name.as_ref().or(Some(&job_name)).cloned();
    }

    pub(crate) fn write_slurm_config<W: Write>(&self, mut writer: W) -> Result<(), std::io::Error> {
        slurm_helper!(self, writer;
            "job-name"=>job_name,
            "output"=>output,
            "nodes"=>nodes,
            "ntasks"=>ntasks,
            "cpus-per-task"=>cpus_per_task,
            "mem-per-cpu"=>mem_per_cpu,
            "hint"=>hint,
            "time"=>time,
            "partition"=>partition,
            "account"=>account,
            "mail-user"=>mail_user,
            "mail-type"=>mail_type
        );

        Ok(())
    }
}

#[test]
fn serialize_nodes() {
    let bytes = include_str!("../../static/example-nodes.yaml");
    let _out: Nodes = serde_yaml::from_str(bytes).unwrap();
}

#[test]
fn serialize_jobs_python() {
    let bytes = include_str!("../../static/example-jobs-python.yaml");
    let _out: Jobs<common::File> = serde_yaml::from_str(bytes).unwrap();
}

#[test]
fn serialize_jobs_apptainer() {
    let bytes = include_str!("../../static/example-jobs-apptainer.yaml");
    let _out: Jobs<common::File> = serde_yaml::from_str(bytes).unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialize_python() {
        let bytes = include_str!("../../static/example-jobs-python.yaml");
        let _out: super::PythonConfig<common::File> = serde_yaml::from_str(bytes).unwrap();
    }

    #[test]
    fn serialize_apptainer() {
        let bytes = include_str!("../../static/example-jobs-apptainer.yaml");
        let _out: ApptainerConfig<common::File> = serde_yaml::from_str(bytes).unwrap();
    }

    #[test]
    fn strings_match() {
        assert_eq!(SERVER_PORT.to_string(), SERVER_PORT_STR.to_string());
    }
}
