use crate::config;
use derive_more::{Constructor, Display, From};
use std::io;
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Could not load configuration file. {0}")]
    InvalidConfiguration(#[from] config::ConfigurationError),
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
    #[error("Failed to setup logging: {0}")]
    Log(#[from] LogError),
    #[error("{0}")]
    ClientInit(#[from] ClientInitError),
    #[error("{0}")]
    Pause(#[from] PauseError),
    #[error("{0}")]
    Add(#[from] AddError),
    #[error("{0}")]
    Status(#[from] StatusError),
    #[error("Error when building a job: {0}")]
    BuildJob(#[from] BuildJobError),
    #[error("Error when executing a job on a node: {0}")]
    RunningJob(#[from] RunningNodeError),
    #[error("{0}")]
    Template(#[from] TemplateError),
    #[error("{0}")]
    PullErrorLocal(#[from] PullErrorLocal),
    #[error("{0}")]
    Timeout(#[from] TimeoutError),
}

#[derive(thiserror::Error, Debug, From)]
pub enum TcpConnection {
    #[error("A general io error occured when reading from a TCP connection: `{0}`")]
    General(std::io::Error),
    #[error("the tcp connection has been closed by the other party")]
    ConnectionClosed,
    #[error("{0}")]
    ParseAddress(AddressParseError),
    #[error("TCP deserialization error: {0}")]
    Deser(Deserialization),
    #[error("TCP serialization error {0}")]
    Ser(Serialization),
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
    #[error(
        "could not create / edit the job run script file (this should not happen). Error: `{0}`"
    )]
    CreateFile(CreateFile),
    #[error("{0}")]
    CreateDir(CreateDir),
    #[error("could not open file `{path}`, full error: {full_error}")]
    OpenFile {
        path: std::path::PathBuf,
        full_error: std::io::Error,
    },
    #[error("{0}")]
    ReadBytes(ReadBytes),
    #[error("could not write bytes`{path}`, full error: {full_error}")]
    WriteBytes {
        path: std::path::PathBuf,
        full_error: std::io::Error,
    },
    #[error("{0}")]
    ExecuteProcess(CommandExecutionError),
    #[error("{0}")]
    RenameFile(#[from] RenameFile),
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
    #[error("error: One node failed to respond correctly. The server will not start")]
    MissingNode,
    #[error("{0}")]
    LoadJobs(config::LoadJobsError),
    #[error("{0}")]
    ParseIp(ParseIpError),
    #[error("{0}")]
    WriteFile(WriteFile),
    #[error("{0}")]
    CreateDir(CreateDir),
    #[error("{0}")]
    RemoveDir(RemoveDirError),
    #[error("{0}")]
    OpenFile(OpenFile),
    #[error("{0}")]
    SerializeConfig(SerializeConfig),
}

#[derive(Debug, Display, From, thiserror::Error)]
#[display(
    fmt = "Error loading configuration from nodes (`{}`): {} ",
    configuration_error,
    path
)]
pub struct NodesConfigError {
    configuration_error: config::ConfigurationError,
    path: String,
}

#[derive(Debug, Display, From, thiserror::Error, Constructor)]
#[display(fmt = "Failed to open file {} - error: {}", "path.display()", error)]
pub struct OpenFile {
    error: std::io::Error,
    path: PathBuf,
}

#[derive(Debug, Display, From, thiserror::Error, Constructor)]
#[display(
    fmt = "Failed to open create file {} - error: {}",
    "path.display()",
    error
)]
pub struct CreateFile {
    error: std::io::Error,
    path: PathBuf,
}

#[derive(Debug, Display, From, thiserror::Error, Constructor)]
#[display(
    fmt = "failed to rename file from `{}` to `{}`: {error}",
    "src.display()",
    "dest.display()"
)]
pub struct RenameFile {
    error: std::io::Error,
    src: PathBuf,
    dest: PathBuf,
}

#[derive(Debug, Display, From, thiserror::Error, Constructor)]
#[display(
    fmt = "Could not serialize config file at {} - error: {}",
    "path.display()",
    error
)]
pub struct SerializeConfig {
    error: serde_json::Error,
    path: PathBuf,
}

#[derive(Debug, Display, From, thiserror::Error)]
#[display(
    fmt = "Error loading configuration for jobs (`{}`): {} ",
    configuration_error,
    path
)]
pub struct JobsConfigError {
    configuration_error: config::ConfigurationError,
    path: String,
}

#[derive(Debug, Display, From, thiserror::Error)]
#[display(fmt = "Could not map ip address {} to a SocketAddr", ip)]
/// error that happens when loading the bytes of a job file from path
pub struct ParseIpError {
    ip: String,
}

#[derive(Debug, Display, From, thiserror::Error, Constructor)]
#[display(fmt = "Could not write to file {:?}, error: {}", path, error)]
pub struct WriteFile {
    error: std::io::Error,
    path: PathBuf,
}

#[derive(Debug, Display, From, thiserror::Error, Constructor)]
#[display(
    fmt = "Failed to read the bytes for file {} - error: {}",
    "path.display()",
    error
)]
pub struct ReadBytes {
    error: std::io::Error,
    path: PathBuf,
}

#[derive(Debug, Display, From, Constructor, thiserror::Error)]
#[display(fmt = "Could create directory {:?}, error: {}", path, error)]
pub struct CreateDir {
    error: std::io::Error,
    path: PathBuf,
}

#[derive(Debug, Display, Constructor, thiserror::Error)]
#[display(fmt = "Could create remove directory `{:?}`, error: {}", path, error)]
pub struct RemoveDirError {
    error: std::io::Error,
    path: PathBuf,
}

#[derive(Debug, From, thiserror::Error)]
pub enum LogError {
    #[error("`{0}`")]
    Io(std::io::Error),
    #[error("`{0}`")]
    CreateFile(CreateFile),
}

#[derive(Debug, From, thiserror::Error)]
pub enum ClientInitError {
    #[error("`{0}`")]
    Io(std::io::Error),
    #[error("`{0}`")]
    RenameFile(RenameFile),
    #[error("`{0}`")]
    TcpConnection(TcpConnection),
    #[error("`{0}`")]
    CreateDir(CreateDir),
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
    #[error("Error when accessing some unix filesystems or commands: `{0}`")]
    Unix(UnixError),
}

#[derive(Debug, From, thiserror::Error)]
pub enum AddError {
    #[error("Server did not send capabilities when requested. Instead, sent `{0}`")]
    NotCapabilities(crate::transport::ServerResponseToUser),
    #[error("None of the nodes could run the jobs. Aborted")]
    NoCompatableNodes,
    #[error("Could not add the job set on the server side. This is generally a really really bad error. You should tell brooks about this.")]
    FailedToAdd,
    #[error("Failed to send jobs to the server")]
    FailedSend,
    #[error("There were no actual jobs specified in the configuration file")]
    NoJobsToAdd,
    #[error("Duplicate job name `{0}` appeared in config file. Job names must be unique")]
    DuplicateJobName(String),
    #[error("{0}")]
    MissingFilename(config::MissingFileNameError),
    #[error("{0}")]
    ConfigError(config::ConfigErrorReason),
}

#[derive(Debug, From, thiserror::Error)]
pub enum StatusError {
    #[error("Server did not send job list as requested. Instead, sent `{0}`")]
    NotQueryJobs(crate::transport::ServerResponseToUser),
}

#[derive(Debug, From, thiserror::Error)]
pub enum ScheduleError {
    #[error("{0}")]
    StoreSet(StoreSet),
}

#[derive(Debug, Display, From, Constructor, thiserror::Error)]
#[display(fmt = "Failed to save job set to memory: {}", error)]
pub struct StoreSet {
    error: std::io::Error,
}

#[derive(Debug, From, thiserror::Error)]
pub enum BuildJobError {
    #[error("{0}")]
    CreateDirector(CreateDir),
    #[error("{0}")]
    Other(Box<Error>),
    #[error("The scheduler returned a runnable job instead of an initialization job when we first requested a task.")]
    SentExecutable,
}

#[derive(Debug, From, thiserror::Error)]
pub enum RunningNodeError {
    #[error("{0}")]
    CreateDirector(CreateDir),
    #[error("{0}")]
    Other(Box<Error>),
    #[error("The job returned to us has not been built before")]
    MissingBuildStep,
}

#[derive(Debug, From, thiserror::Error)]
pub enum UnixError {
    #[error("Io error reading a file: `{0}`")]
    Io(std::io::Error),
}

#[derive(Debug, From, thiserror::Error)]
pub enum TemplateError {
    #[error("Could not serialize the template to a file: {0}")]
    Serde(serde_yaml::Error),
    #[error("Could not write to the output file: {0}")]
    Io(io::Error),
    #[error("failed to load some information for the job: {0}")]
    LoadJobs(crate::config::LoadJobsError),
}

#[derive(Debug, From, thiserror::Error, serde::Deserialize, Clone, serde::Serialize)]
pub enum PullError {
    #[error("The namespace requested does not exist")]
    MissingNamespace,
    #[error("The batch name within the namespace requested does not exist")]
    MissingBatchname,
    #[error("Failed to load file for sending: `{0}`")]
    LoadFile(PathBuf),
}

#[derive(Debug, From, thiserror::Error)]
pub enum PullErrorLocal {
    #[error("Error with configuration file: {0}")]
    Config(config::ConfigurationError),
    #[error("{0}")]
    Regex(RegexError),
    #[error("Unexpected resposne from the server")]
    UnexpectedResponse,
    #[error("{0}")]
    CreateDir(CreateDir),
    #[error("{0}")]
    WriteFile(WriteFile),
}

#[derive(Debug, Display, thiserror::Error, Constructor)]
#[display(fmt = "regular expression: `{}` error reason: `{}`", "expr", "error")]
pub struct RegexError {
    expr: String,
    error: regex::Error,
}

#[derive(Debug, From, thiserror::Error)]
pub enum RunErrorLocal {
    #[error("Error with configuration file: {0}")]
    Config(config::ConfigurationError),
    #[error("Unexpected resposne from the server")]
    UnexpectedResponse,
    #[error("{0}")]
    CreateDir(CreateDir),
    #[error("{0}")]
    WriteFile(WriteFile),
    #[error("the specified --save_dir folder exists and --clean-save was not specifed")]
    FolderExists,
    #[error("A general io error occured: {0}")]
    GeneralIo(std::io::Error),
    #[error("A python configuration was specified, but run-local only supports apptainer configurations")]
    OnlyApptainer,
    #[error("{0}")]
    LoadJobs(config::LoadJobsError),
    #[error("{0}")]
    GeneralError(Error),
}

#[derive(Debug, Display, thiserror::Error, Constructor)]
#[display(
    fmt = "Node at {} /  {} has timed out a keepalive connection",
    "name",
    "addr"
)]
pub struct TimeoutError {
    addr: std::net::SocketAddr,
    name: String,
}

#[derive(Debug, Display, thiserror::Error, Constructor)]
#[display(fmt = "`{executable}` executable could not be found in `PATH`: `{err}`")]
/// Apptainer executable was not found
pub struct ExecutableMissing {
    executable: String,
    err: std::io::Error,
}
