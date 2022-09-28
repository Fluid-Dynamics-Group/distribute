use derive_more::Constructor;
use std::net::IpAddr;
use std::path::PathBuf;
use structopt::StructOpt;

use crate::config::{
    CLIENT_CANCEL_PORT_STR, CLIENT_KEEPALIVE_PORT_STR, CLIENT_PORT_STR, SERVER_PORT_STR,
};

#[derive(StructOpt, PartialEq, Debug, Eq)]
#[structopt(
    name = "distribute", 
    about = "A utility for scheduling jobs on a cluster", 
    version=env!("CARGO_PKG_VERSION")
)]
pub struct ArgsWrapper {
    #[structopt(long)]
    pub save_log: bool,

    #[structopt(long)]
    pub show_logs: bool,

    #[structopt(subcommand)]
    pub command: Arguments,
}

#[derive(StructOpt, PartialEq, Debug, Eq)]
pub enum Arguments {
    Client(Client),
    Server(Server),
    Kill(Kill),
    Pause(Pause),
    Add(Add),
    Template(Template),
    Pull(Pull),
    Run(Run),
    ServerStatus(ServerStatus),
    NodeStatus(NodeStatus),
}

impl Arguments {
    pub fn log_path(&self) -> PathBuf {
        match &self {
            Self::Client(c) => c.log_file.clone(),
            _ => "./output.log".into(),
        }
    }
}

#[derive(StructOpt, PartialEq, Debug, Constructor, Eq)]
/// start this workstation as a node and prepare it for a server connection
pub struct Client {
    /// location where all compilation and running takes place. all
    /// job init stuff will be done here
    pub base_folder: PathBuf,

    #[structopt(long, default_value = CLIENT_PORT_STR, short="p")]
    /// the port to bind the client to
    pub transport_port: u16,

    #[structopt(long, default_value = CLIENT_KEEPALIVE_PORT_STR, short)]
    /// the port for client to bind for keepalive checks
    pub keepalive_port: u16,

    #[structopt(long, default_value = CLIENT_CANCEL_PORT_STR, short)]
    /// port to receive cancelation messages on
    pub cancel_port: u16,

    #[structopt(long, default_value = "./output.log", short)]
    /// the port to bind the client to (default 8953)
    pub log_file: PathBuf,
}

#[derive(StructOpt, PartialEq, Debug, Constructor, Eq)]
/// start serving jobs out to nodes using the provied configuration file
pub struct Server {
    #[structopt(long, default_value = "distribute-nodes.yaml")]
    /// the path to the yaml file describing all available nodes
    pub nodes_file: PathBuf,

    #[structopt(long)]
    /// directory where all files sent by nodes are saved
    pub save_path: std::path::PathBuf,

    #[structopt(long)]
    /// all stored files sent to the server saved to
    pub temp_dir: std::path::PathBuf,

    #[structopt(long, default_value = SERVER_PORT_STR, short)]
    /// the port to bind the server to (default 8952)
    pub port: u16,

    #[structopt(long, short)]
    /// clean and remove the entire output tree
    pub clean_output: bool,

    #[structopt(long, short)]
    /// api keys for matrix messages
    pub matrix_config: Option<std::path::PathBuf>,
}

#[derive(StructOpt, PartialEq, Debug, Constructor, Eq)]
/// check the status of all the nodes
pub struct ServerStatus {
    #[structopt(long, short, default_value = SERVER_PORT_STR)]
    /// the port that the server uses (default 8952)
    pub port: u16,

    #[structopt(long)]
    /// the ip address that the server is located at
    pub ip: IpAddr,
}

#[derive(StructOpt, PartialEq, Debug, Constructor, Eq)]
/// check the status of all the nodes
pub struct NodeStatus {
    #[structopt(long, default_value = "distribute-nodes.yaml")]
    /// the path to the yaml file describing all available nodes
    pub nodes_file: PathBuf,
}

#[derive(StructOpt, PartialEq, Debug, Eq)]
/// terminate any running jobs of a given batch name and remove the batch from the queue
pub struct Kill {
    #[structopt(long, short, default_value = SERVER_PORT_STR)]
    /// the port that the server uses (default 8952)
    pub port: u16,

    #[structopt(long)]
    /// the ip address that the server is located at
    pub ip: IpAddr,

    /// the name of the job to kill
    pub job_name: String,
}

#[derive(StructOpt, PartialEq, Debug, Eq)]
/// pause all currently running processes on this node for a specified amount of time
pub struct Pause {
    #[structopt(long, default_value = "1h")]
    /// duration to pause the processes for.  Maximum allowable
    /// pause time is 4 hours. (Examples: 1h, 90m, 1h30m, 1m30s).
    ///
    /// This command requires sudo.
    pub duration: String,
}

#[derive(StructOpt, PartialEq, Debug, Constructor, Eq)]
/// add a job set to the queue
pub struct Add {
    #[structopt(default_value = "distribute-jobs.yaml")]
    pub jobs: PathBuf,

    #[structopt(long, short, default_value = SERVER_PORT_STR)]
    /// the port that the server uses (default 8952)
    pub port: u16,

    #[structopt(long)]
    /// the ip address that the server is located at
    pub ip: IpAddr,

    #[structopt(long, short)]
    /// print out the capabilities of each node
    pub show_caps: bool,

    #[structopt(long, short)]
    /// execute as normal but don't send the job set to the server
    pub dry: bool,
}

#[derive(StructOpt, PartialEq, Debug, Eq)]
/// generate a template file to fill for executing with `distribute add`
pub struct Template {
    #[structopt(subcommand)]
    /// set the configuration type to either python or apptainer format
    pub(crate) mode: TemplateType,

    #[structopt(long, default_value = "distribute-jobs.yaml")]
    /// an optional path to write the template result to
    pub output: PathBuf,
}

#[derive(StructOpt, PartialEq, Debug, Eq)]
pub(crate) enum TemplateType {
    Apptainer,
    Python,
}

#[derive(StructOpt, PartialEq, Debug, Constructor, Eq)]
/// Pull files from the server to your machine
pub struct Pull {
    #[structopt(long)]
    /// the ip address that the server is located at
    pub ip: IpAddr,

    #[structopt(default_value = "distribute-jobs.yaml")]
    pub(crate) job_file: PathBuf,

    #[structopt(long, short)]
    /// Whether or not to only check what files _would_ be downloaded
    /// with the provided regular expressions
    pub(crate) dry: bool,

    #[structopt(long, short, default_value=SERVER_PORT_STR)]
    /// The port of the server to connect to
    pub(crate) port: u16,

    #[structopt(long, short, default_value = "./")]
    pub(crate) save_dir: PathBuf,

    #[structopt(subcommand)]
    pub(crate) filter: Option<RegexFilter>,
}

#[derive(StructOpt, PartialEq, Debug, Eq)]
pub enum RegexFilter {
    /// files to include in the pulling operation
    Include {
        #[structopt(long, short)]
        include: Vec<String>,
    },
    /// files to exlclude in the pulling operation
    Exclude {
        #[structopt(long, short)]
        exclude: Vec<String>,
    },
}

#[derive(StructOpt, PartialEq, Debug, Constructor, Eq)]
/// run a apptainer configuration file locally (without sending it off to a server)
pub struct Run {
    #[structopt(default_value = "distribute-jobs.yaml")]
    /// location of your configuration file
    pub(crate) job_file: PathBuf,

    #[structopt(long, short, default_value = "./distribute-run")]
    /// the directory where all the work will be performed
    pub(crate) save_dir: PathBuf,

    #[structopt(long)]
    /// allow the save_dir to exist, but remove all the contents
    /// of it before executing the code
    pub(crate) clean_save: bool,
}

fn check_send<T: Send>() {}

fn other_send() {
    check_send::<Server>();
}
