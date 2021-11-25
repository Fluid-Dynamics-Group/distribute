use structopt::StructOpt;
use std::net::IpAddr;
use std::path::PathBuf;

pub const SERVER_PORT: u16 = 8952;
pub const CLIENT_PORT: u16 = 8953;

//#[derive(StructOpt, PartialEq, Debug)]
///// distribute compute jobs between multiple machines
//pub struct Arguments {
//    #[structopt(subcommand)]
//    pub(crate) command: Command,
//}

#[derive(StructOpt, PartialEq, Debug)]
#[structopt(name = "distribute", about = "A utility for scheduling jobs on a cluster")]
pub enum Arguments {
    Client(Client),
    Server(Server),
    Status(Status),
    Kill(Kill),
    Pause(Pause),
    Add(Add),
    Template(Template),
    Pull(Pull)
}

#[derive(StructOpt, PartialEq, Debug)]
/// start this workstation as a node and prepare it for a server connection
pub struct Client {
    /// location where all compilation and running takes place. all
    /// job init stuff will be done here
    pub base_folder: String,

    #[structopt(long, default_value = "CLIENT_PORT", short)]
    /// the port to bind the client to (default 8953)
    pub port: u16,
}

#[derive(StructOpt, PartialEq, Debug)]
/// start serving jobs out to nodes using the provied configuration file
pub struct Server {
    #[structopt(long, default_value = "distribute-nodes.yaml")]
    /// the path to the yaml file describing all available nodes
    pub nodes_file: String,

    #[structopt(long)]
    /// directory where all files sent by nodes are saved
    pub save_path: std::path::PathBuf,

    #[structopt(long)]
    /// all stored files sent to the server saved to
    pub temp_dir: std::path::PathBuf,

    #[structopt(long, default_value = "SERVER_PORT", short)]
    /// the port to bind the server to (default 8952)
    pub port: u16,

    #[structopt(long,short)]
    /// clean and remove the entire output tree
    pub clean_output: bool,
}

#[derive(StructOpt, PartialEq, Debug)]
/// check the status of all the nodes
pub struct Status {
    #[structopt(long, short, default_value = "SERVER_PORT")]
    /// the port that the server uses (default 8952)
    pub port: u16,

    #[structopt(long)]
    /// the ip address that the server is located at
    pub ip: IpAddr,
}

#[derive(StructOpt, PartialEq, Debug)]
/// check the status of all the nodes
pub struct Kill {
    #[structopt(long,short, default_value = "SERVER_PORT")]
    /// the port that the server uses (default 8952)
    pub port: u16,

    #[structopt(long)]
    /// the ip address that the server is located at
    pub ip: IpAddr,

    /// the name of the job to kill
    pub job_name: String
}

#[derive(StructOpt, PartialEq, Debug)]
/// pause all currently running processes on this node for a specified amount of time. If
/// no processes are running then the command is ignored by the node
pub struct Pause {
    #[structopt(long, default_value = "1h")]
    /// duration to pause the processes for.  Maximum allowable
    /// pause time is 4 hours. (Examples: 1h, 90m, 1h30m).
    pub duration: String,
}

#[derive(StructOpt, PartialEq, Debug)]
/// add a job set to the queue
pub struct Add {
    #[structopt(default_value = "distribute-jobs.yaml")]
    pub jobs: String,

    #[structopt(long,short , default_value = "SERVER_PORT")]
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

#[derive(StructOpt, PartialEq, Debug)]
/// add a job set to the queue
pub struct Template {
    #[structopt(subcommand)]
    /// set the configuration type to either python or singularity format
    pub(crate) mode: TemplateType,

    #[structopt(long, default_value ="distribute-jobs.yaml")]
    /// an optional path to write the template result to
    pub output: PathBuf
}

#[derive(StructOpt, PartialEq, Debug)]
pub(crate) enum TemplateType {
    Singularity,
    Python
}

#[derive(StructOpt, PartialEq, Debug)]
/// Pull files from the server to your machine
pub struct Pull {
    #[structopt(long)]
    /// the ip address that the server is located at
    pub ip: IpAddr,

    #[structopt(long)]
    /// files to include in the pulling operation
    pub include: Option<String>
}
