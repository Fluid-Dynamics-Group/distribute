use derive_more::Constructor;
use std::net::IpAddr;
use std::path::PathBuf;

use clap::Parser;
use clap::Subcommand;

use crate::config::{
    CLIENT_CANCEL_PORT_STR, CLIENT_KEEPALIVE_PORT_STR, CLIENT_PORT_STR, SERVER_PORT_STR,
};

#[derive(Parser, PartialEq, Debug, Eq)]
#[clap(
    name = "distribute", 
    about = "A utility for scheduling jobs on a cluster", 
    version=env!("CARGO_PKG_VERSION")
)]
/// General CLI handler for all user inputs
pub struct ArgsWrapper {
    #[arg(long)]
    /// save the logs to a file
    pub save_log: bool,

    #[arg(long)]
    /// print logs to stdout
    pub show_logs: bool,

    #[clap(subcommand)]
    /// general
    pub command: Arguments,
}

#[derive(Subcommand, PartialEq, Debug, Eq)]
/// top-level subcommand handler
pub enum Arguments {
    /// start a client / compute node
    Client(Client),
    /// start a server / head node
    Server(Server),
    /// kill a jobset on the server
    Kill(Kill),
    /// pause jobs running locally
    Pause(Pause),
    /// add a job set to a head node
    Add(Add),
    /// generate a distribute-nodes.yaml template
    Template(Template),
    /// pull archived results from the server
    Pull(Pull),
    /// run an apptainer job locally
    Run(Run),
    /// check the status of the server, including
    /// currently executing jobs and jobs
    /// remaining in each job batch
    ServerStatus(ServerStatus),
    /// (server command) to check that all nodes
    /// are up and they have the correct versions of distribute
    NodeStatus(NodeStatus),
    /// transpile a distribute compute config to run on slurm
    Slurm(Slurm),
}

impl Arguments {
    /// helper function to determine the path to place a log file
    pub fn log_path(&self) -> PathBuf {
        match &self {
            Self::Client(c) => c.log_file.clone(),
            _ => "./output.log".into(),
        }
    }
}

#[derive(Parser, PartialEq, Debug, Constructor, Eq)]
/// start this workstation as a node and prepare it for a server connection
pub struct Client {
    /// location where all compilation and running takes place. all
    /// job init stuff will be done here
    pub base_folder: PathBuf,

    #[arg(long, default_value = CLIENT_PORT_STR, short='p')]
    /// the port to bind the client to
    pub transport_port: u16,

    #[arg(long, default_value = CLIENT_KEEPALIVE_PORT_STR, short)]
    /// the port for client to bind for keepalive checks
    pub keepalive_port: u16,

    #[arg(long, default_value = CLIENT_CANCEL_PORT_STR, short)]
    /// port to receive cancelation messages on
    pub cancel_port: u16,

    #[arg(long, default_value = "./output.log", short)]
    /// the port to bind the client to (default 8953)
    pub log_file: PathBuf,
}

#[derive(Parser, PartialEq, Debug, Constructor, Eq)]
/// start serving jobs out to nodes using the provied configuration file
pub struct Server {
    #[arg(long, default_value = "distribute-nodes.yaml")]
    /// the path to the yaml file describing all available nodes
    pub nodes_file: PathBuf,

    #[arg(long)]
    /// directory where all files sent by nodes are saved
    pub save_path: std::path::PathBuf,

    #[arg(long)]
    /// all stored files sent to the server saved to
    pub temp_dir: std::path::PathBuf,

    #[arg(long, default_value = SERVER_PORT_STR, short)]
    /// the port to bind the server to (default 8952)
    pub port: u16,

    #[arg(long, short)]
    /// clean and remove the entire output tree
    pub clean_output: bool,

    #[arg(long, short)]
    /// api keys for matrix messages
    pub matrix_config: Option<std::path::PathBuf>,
}

#[derive(Parser, PartialEq, Debug, Constructor, Eq)]
/// check the status of all the nodes
pub struct ServerStatus {
    #[arg(long, short, default_value = SERVER_PORT_STR)]
    /// the port that the server uses (default 8952)
    pub port: u16,

    #[arg(long)]
    /// the ip address that the server is located at
    pub ip: IpAddr,
}

#[derive(Parser, PartialEq, Debug, Constructor, Eq)]
/// check the status of all the nodes
pub struct NodeStatus {
    #[arg(long, default_value = "distribute-nodes.yaml")]
    /// the path to the yaml file describing all available nodes
    pub nodes_file: PathBuf,
}

#[derive(Parser, PartialEq, Debug, Eq)]
/// terminate any running jobs of a given batch name and remove the batch from the queue
pub struct Kill {
    #[arg(long, short, default_value = SERVER_PORT_STR)]
    /// the port that the server uses (default 8952)
    pub port: u16,

    #[arg(long)]
    /// the ip address that the server is located at
    pub ip: IpAddr,

    /// the name of the job to kill
    pub job_name: String,
}

#[derive(Parser, PartialEq, Debug, Eq)]
/// pause all currently running processes on this node for a specified amount of time
pub struct Pause {
    #[arg(long, default_value = "1h")]
    /// duration to pause the processes for.  Maximum allowable
    /// pause time is 4 hours. (Examples: 1h, 90m, 1h30m, 1m30s).
    ///
    /// This command requires sudo.
    pub duration: String,
}

#[derive(Parser, PartialEq, Debug, Constructor, Eq)]
/// add a job set to the queue
pub struct Add {
    #[arg(default_value = "distribute-jobs.yaml")]
    /// distribute-jobs.yaml configuration file
    pub jobs: PathBuf,

    #[arg(long, short, default_value = SERVER_PORT_STR)]
    /// the port that the server uses (default 8952)
    pub port: u16,

    #[arg(long)]
    /// the ip address that the server is located at
    pub ip: IpAddr,

    #[arg(long, short)]
    /// print out the capabilities of each node
    pub show_caps: bool,

    #[arg(long, short)]
    /// execute as normal but don't send the job set to the server
    pub dry: bool,
}

#[derive(Parser, PartialEq, Debug, Eq)]
/// generate a template file to fill for executing with `distribute add`
pub struct Template {
    #[command(subcommand)]
    /// set the configuration type to either python or apptainer format
    pub(crate) mode: TemplateType,

    #[arg(long, default_value = "distribute-jobs.yaml")]
    /// an optional path to write the template result to
    pub output: PathBuf,
}

#[derive(Subcommand, PartialEq, Debug, Eq)]
pub(crate) enum TemplateType {
    Apptainer,
    Python,
}

#[derive(Parser, PartialEq, Debug, Constructor, Eq)]
/// Pull files from the server to your machine
pub struct Pull {
    #[arg(long)]
    /// the ip address that the server is located at
    pub ip: IpAddr,

    #[arg(default_value = "distribute-jobs.yaml")]
    pub(crate) job_file: PathBuf,

    #[arg(long, short)]
    /// Whether or not to only check what files _would_ be downloaded
    /// with the provided regular expressions
    pub(crate) dry: bool,

    #[arg(long, short)]
    /// dont create full folder structure when pulling a subset of files. This
    /// option can only be used /after/ the full folder structure has been
    /// created by a previous `distribute pull` command that did not use this
    /// option.
    pub(crate) skip_folders: bool,

    #[arg(long, short, default_value=SERVER_PORT_STR)]
    /// The port of the server to connect to
    pub(crate) port: u16,

    #[arg(long, short, default_value = "./")]
    pub(crate) save_dir: PathBuf,

    #[command(subcommand)]
    pub(crate) filter: Option<RegexFilter>,
}

#[derive(Parser, PartialEq, Debug, Eq)]
/// A filter applied to pull queries to the server.
///
/// Queries can either be "include" based (only include files specified), or
/// exclude based (pull all files except those matching exclude query)
pub enum RegexFilter {
    /// files to include in the pulling operation. all --include flags are included with an OR
    /// basis.
    ///
    /// --include "file_1" --include "file_2" will include matches for both file_1 or file_2
    Include {
        #[structopt(long, short)]
        /// include fields
        include: Vec<String>,
    },
    /// files to exlclude in the pulling operation. all --exclude flags are included with an OR
    /// basis.
    ///
    /// --exclude "file_1" --exclude "file_2" will exclude matches for both file_1 or file_2
    Exclude {
        #[structopt(long, short)]
        /// exclude fields
        exclude: Vec<String>,
    },
}

#[derive(Parser, PartialEq, Debug, Constructor, Eq)]
/// run a apptainer configuration file locally (without sending it off to a server)
pub struct Run {
    #[arg(default_value = "distribute-jobs.yaml")]
    /// location of your configuration file
    pub(crate) job_file: PathBuf,

    #[arg(long, short, default_value = "./distribute-run")]
    /// the directory where all the work will be performed
    pub(crate) save_dir: PathBuf,

    #[arg(long)]
    /// allow the save_dir to exist, but remove all the contents
    /// of it before executing the code
    pub(crate) clean_save: bool,
}

fn check_send<T: Send>() {}

fn other_send() {
    check_send::<Server>();
}

#[derive(Parser, PartialEq, Debug, Constructor, Eq)]
/// transpile a set of jobs in `distribute-jobs.yaml` to be run on a SLURM cluster
pub struct Slurm {
    /// path to the folder that will be created to store the assimilated data to be shipped
    /// off to the cluster
    pub(crate) output_folder: PathBuf,

    #[arg(default_value = "distribute-jobs.yaml")]
    /// distribute-jobs.yaml configuration file
    pub(crate) jobs: PathBuf,

    #[arg(long)]
    /// the username that will be used to transfer the files to over ssh.
    pub(crate) cluster_username: String,

    #[arg(long)]
    /// address of the cluster.
    ///
    /// Ex: pronghorn.rc.unr.edu
    pub(crate) cluster_address: String,

    #[arg(long)]
    /// folder that the files will placed in.
    ///
    /// Ex: /home/your_username/another/
    pub(crate) cluster_upload_destination: PathBuf,
}
