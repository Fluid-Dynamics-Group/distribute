use derive_more::{Constructor, Display, From, Unwrap};
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Could not load configuration file. {0}")]
    InvalidConfiguration(#[from] ConfigurationError),
    #[error("{0}")]
    TcpConnection(#[from] TcpConnection),
    #[error("{0}")]
    TransportSerialization(#[from] Serialization),
    #[error("{0}")]
    TransportDeserialization(#[from] Deserialization),
    #[error("{0}")]
    RunJob(#[from] RunJobError),
    #[error("{0}")]
    InitJob(#[from] InitJobError),
    #[error("{0}")]
    Server(#[from] ServerError),
    #[error("{0}")]
    Log(#[from] LogError),
    #[error("{0}")]
    ClientInit(#[from] ClientInitError),
    #[error("{0}")]
    Pause(#[from] PauseError),
}

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

#[derive(Debug, From, thiserror::Error, Unwrap)]
pub enum TcpConnection {
    #[error("A general io error occured when reading from a TCP connection: `{0}`")]
    General(std::io::Error),
    #[error("the tcp connection has been closed by the other party")]
    ConnectionClosed,
    #[error("{0}")]
    ParseAddress(AddressParseError),
}

#[derive(Debug, Display, From, thiserror::Error)]
#[display(
    fmt = "error serializing data to bytes (this is probably a bug): {}",
    error
)]
pub struct Serialization {
    error: bincode::Error,
}

#[derive(Debug, Display, From, thiserror::Error)]
#[display(
    fmt = "could not parse address from string `{}`. full error: {}",
    ip,
    error
)]
pub struct AddressParseError {
    ip: String,
    error: std::net::AddrParseError,
}

#[derive(Debug, Display, From, thiserror::Error)]
#[display(
    fmt = "error deserializing data to bytes (this is probably a bug): {}",
    error
)]
pub struct Deserialization {
    error: bincode::Error,
}

#[derive(Debug, thiserror::Error)]
pub enum RunJobError {
    #[error("could not create / edit the job run script ({path}) (this should not happen). Error: `{0}`")]
    CreateFile {
        path: std::path::PathBuf,
        full_error: std::io::Error,
    },
    #[error("could not open file `{path}`, full error: {full_error}")]
    OpenFile {
        path: std::path::PathBuf,
        full_error: std::io::Error,
    },
    #[error("could not read bytes`{path}`, full error: {full_error}")]
    ReadBytes {
        path: std::path::PathBuf,
        full_error: std::io::Error,
    },
    #[error("could not write bytes`{path}`, full error: {full_error}")]
    WriteBytes {
        path: std::path::PathBuf,
        full_error: std::io::Error,
    },
    #[error("{0}")]
    ExecuteProcess(CommandExecutionError),
}

#[derive(Debug, Display, From, thiserror::Error)]
#[display(fmt = "could not execute run file command: {}", error)]
pub struct CommandExecutionError {
    error: std::io::Error,
}

#[derive(Debug, From, thiserror::Error)]
pub enum InitJobError {
    #[error("could not create / edit the job run script (this should not happen). Error: `{0}`")]
    CreateFile(std::io::Error),
    #[error("{0}")]
    ExecuteProcess(CommandExecutionError),
    #[error("{0}")]
    RemovePreviousDir(RemovePreviousDir),
}

#[derive(Debug, From, thiserror::Error)]
pub enum ServerError {
    #[error("{0}")]
    NodesConfigError(NodesConfigError),
    #[error("{0}")]
    JobsConfigError(JobsConfigError),
    #[error("error: Cannot automatically infer format for types with more than 1 field")]
    MissingNode,
    #[error("{0}")]
    LoadJobs(LoadJobsError),
    #[error("{0}")]
    ParseIp(ParseIpError),
    #[error("{0}")]
    WriteFile(WriteFile),
    #[error("{0}")]
    CreateDir(CreateDirError),
}

#[derive(Debug, Display, From, thiserror::Error)]
#[display(
    fmt = "Error loading configuration from nodes (`{}`): {} ",
    configuration_error,
    path
)]
pub struct NodesConfigError {
    configuration_error: ConfigurationError,
    path: String,
}

#[derive(Debug, Display, From, thiserror::Error)]
#[display(
    fmt = "Error loading configuration for jobs (`{}`): {} ",
    configuration_error,
    path
)]
pub struct JobsConfigError {
    configuration_error: ConfigurationError,
    path: String,
}

#[derive(Debug, From, thiserror::Error)]
pub enum LoadJobsError {
    #[error("{0}")]
    ReadBytes(ReadBytesError),
    #[error("{0}")]
    MissingFileName(MissingFileNameError),
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

#[derive(Debug, Display, From, thiserror::Error)]
#[display(fmt = "Error loading configuration for jobs {:?} ", path)]
/// happens when a file path does not contain a filename
pub struct MissingFileNameError {
    path: PathBuf,
}

#[derive(Debug, Display, From, thiserror::Error)]
#[display(fmt = "Could not map ip address {} to a SocketAddr", ip)]
/// error that happens when loading the bytes of a job file from path
pub struct ParseIpError {
    ip: String,
}

#[derive(Debug, Display, From, thiserror::Error)]
#[display(fmt = "Could not write to file {:?}, error: {}", path, error)]
pub struct WriteFile {
    error: std::io::Error,
    path: PathBuf,
}

#[derive(Debug, Display, From, thiserror::Error)]
#[display(fmt = "Could create directory {:?}, error: {}", path, error)]
pub struct CreateDirError {
    error: std::io::Error,
    path: PathBuf,
}

#[derive(Debug, From, thiserror::Error)]
pub enum LogError {
    #[error("`{0}`")]
    Io(std::io::Error),
    #[error("`{0}`")]
    SetLogger(log::SetLoggerError),
}

#[derive(Debug, From, thiserror::Error)]
pub enum ClientInitError {
    #[error("`{0}`")]
    Io(std::io::Error),
}

#[derive(Debug, Display, From, Constructor, thiserror::Error)]
#[display(fmt = "failed to wipe output directory at {:?}, error {}", path, error)]
pub struct RemovePreviousDir {
    error: std::io::Error,
    path: PathBuf,
}

#[derive(Debug, From, thiserror::Error)]
pub enum PauseError {
    #[error("`{0}`")]
    ParseString(String),
    #[error(
        "malformed string slice when parsing time input. This is probably a bug or non UTF8 input"
    )]
    BadStringSlice,
    #[error("the character `{0}` is not a valid time identifier. characters should only be h/m/s")]
    InvalidCharacter(char),
    #[error("The input pause duration is too long. Maximum pause duration is 4 hours")]
    DurationTooLong,
}
