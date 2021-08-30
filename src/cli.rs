use argh::FromArgs;

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
    Resume(Resume),
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
    #[argh(option, default = "String::from(\"distribute-jobs.yaml\")")]
    /// the path to the yaml file containing all information on what jobs need to be run
    pub jobs_file: String,

    #[argh(option, default = "String::from(\"distribute-nodes.yaml\")")]
    /// the path to the yaml file describing all available nodes
    pub nodes_file: String,

    #[argh(option)]
    /// directory where all files sent by nodes are saved
    pub save_path: std::path::PathBuf,

    #[argh(option, default = "SERVER_PORT", short = 'p')]
    /// the port to bind the server to (default 8952)
    pub port: u16,
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
    /// pause time is 6 hours. (Examples: 1h, 90m, 1h30m).
    pub duration: String,

    #[argh(option, default = "CLIENT_PORT", short = 'p')]
    /// port that the client is mapped to
    pub port: u16,
}

#[derive(FromArgs, PartialEq, Debug)]
/// resume all processes on this node and undo the pause
#[argh(subcommand, name = "resume")]
pub struct Resume {
    #[argh(option, default = "CLIENT_PORT", short = 'p')]
    /// port that the client is mapped to
    pub port: u16,
}
