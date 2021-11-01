use argh::FromArgs;
use std::net::IpAddr;
use std::path::PathBuf;

pub const SERVER_PORT: u16 = 8952;
pub const CLIENT_PORT: u16 = 8953;

#[derive(FromArgs, PartialEq, Debug)]
/// distribute compute jobs between multiple machines
pub struct Arguments {
    #[argh(subcommand)]
    pub(crate) command: Command,
}

#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand)]
pub enum Command {
    Client(Client),
    Server(Server),
    Status(Status),
    Pause(Pause),
    Add(Add),
    Template(Template)
}

#[derive(FromArgs, PartialEq, Debug)]
/// start this workstation as a node and prepare it for a server connection
#[argh(subcommand, name = "client")]
pub struct Client {
    #[argh(positional)]
    /// location where all compilation and running takes place. all
    /// job init stuff will be done here
    pub base_folder: String,

    #[argh(option, default = "CLIENT_PORT", short = 'p')]
    /// the port to bind the client to (default 8953)
    pub port: u16,
}

#[derive(FromArgs, PartialEq, Debug)]
/// start serving jobs out to nodes using the provied configuration file
#[argh(subcommand, name = "server")]
pub struct Server {
    #[argh(option, default = "String::from(\"distribute-nodes.yaml\")")]
    /// the path to the yaml file describing all available nodes
    pub nodes_file: String,

    #[argh(option)]
    /// directory where all files sent by nodes are saved
    pub save_path: std::path::PathBuf,

    #[argh(option)]
    /// all stored files sent to the server saved to
    pub temp_dir: std::path::PathBuf,

    #[argh(option, default = "SERVER_PORT", short = 'p')]
    /// the port to bind the server to (default 8952)
    pub port: u16,

    #[argh(switch)]
    /// clean and remove the entire output tree
    pub clean_output: bool,
}

#[derive(FromArgs, PartialEq, Debug)]
/// check the status of all the nodes
#[argh(subcommand, name = "status")]
pub struct Status {
    #[argh(positional, default = "String::from(\"distribute-nodes.yaml\")")]
    /// location of the node configuration file that specifies ip addresses of child nodes
    /// that are running
    pub node_information: String,
}

#[derive(FromArgs, PartialEq, Debug)]
/// pause all currently running processes on this node for a specified amount of time. If
/// no processes are running then the command is ignored by the node
#[argh(subcommand, name = "pause")]
pub struct Pause {
    #[argh(option, default = "String::from(\"1h\")")]
    /// duration to pause the processes for.  Maximum allowable
    /// pause time is 4 hours. (Examples: 1h, 90m, 1h30m).
    pub duration: String,
}

#[derive(FromArgs, PartialEq, Debug)]
/// add a job set to the queue
#[argh(subcommand, name = "add")]
pub struct Add {
    #[argh(positional, default = "String::from(\"distribute-jobs.yaml\")")]
    pub jobs: String,

    #[argh(option, default = "SERVER_PORT", short = 'p')]
    /// the port that the server uses (default 8952)
    pub port: u16,

    #[argh(option)]
    /// the ip address that the server is located at
    pub ip: IpAddr,

    #[argh(switch)]
    /// print out the capabilities of each node
    pub show_caps: bool,

    #[argh(switch)]
    /// execute as normal but don't send the job set to the server
    pub dry: bool,
}

#[derive(FromArgs, PartialEq, Debug)]
/// add a job set to the queue
#[argh(subcommand, name = "template")]
pub struct Template {
    #[argh(switch)]
    /// generate a singularity configuration template (default)
    pub singularity: bool,

    #[argh(switch)]
    /// generate a python configuration template
    pub python: bool,

    #[argh(option, default="std::path::PathBuf::from(\"distribute-jobs.yaml\")")]
    /// an optional path to write the template result to
    pub output: PathBuf
}
