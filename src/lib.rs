#![doc = include_str!("../README.md")]
#![allow(dead_code)]
#![deny(missing_docs)]

mod config;

#[cfg(feature = "cli")]
mod add;

#[cfg(feature = "cli")]
/// command line interface helpers and data types
pub mod cli;

#[cfg(feature = "cli")]
mod client;
#[cfg(feature = "cli")]
mod error;
#[cfg(feature = "cli")]
mod kill;
#[cfg(feature = "cli")]
mod node_status;
#[cfg(feature = "cli")]
mod pause;
#[cfg(feature = "cli")]
mod prelude;
#[cfg(feature = "cli")]
mod protocol;
#[cfg(feature = "cli")]
mod pull;
#[cfg(feature = "cli")]
mod run_local;
#[cfg(feature = "cli")]
mod server;
#[cfg(feature = "cli")]
mod server_status;
#[cfg(feature = "cli")]
mod slurm;
#[cfg(feature = "cli")]
mod template;
#[cfg(feature = "cli")]
mod transport;
#[cfg(feature = "cli")]
mod logging;

#[cfg(feature = "cli")]
use prelude::*;

#[cfg(feature = "cli")]
pub use error::{CreateFile, Error, LogError, RunErrorLocal};

#[macro_use]
#[cfg(feature = "cli")]
extern crate tracing;

pub use config::*;

pub use serde_yaml;

pub use matrix_notify::{OwnedUserId, UserId};

#[cfg(feature = "cli")]
pub use {
    add::add, client::client_command, kill::kill, node_status::node_status, pause::pause,
    pull::pull, run_local::run_local, server::server_command, server::RemainingJobs,
    server_status::get_current_jobs, server_status::server_status, slurm::slurm,
    template::template,
};

#[cfg(test)]
mod reexports {
    use super::cli;
    use super::pull;
    use super::server;

    /// helper command to start a server with a boxed error output
    pub async fn start_server(args: cli::Server) -> Result<(), Box<dyn std::error::Error>> {
        server::server_command(args).await?;
        Ok(())
    }

    /// helper command to start pull command with a boxed error output
    pub async fn start_pull(args: cli::Pull) -> Result<(), Box<dyn std::error::Error>> {
        pull::pull(args).await?;
        Ok(())
    }
}

// helper function to setup logging in some integration tests
#[cfg(feature = "cli")]
/// create a logger instance sending output only to stdout
pub fn logger() {
    logging::logger_cfg(logging::LoggingOutput::Stdout, true, true);
}


#[cfg(test)]
/// get a local address at a given port. Used exclusively for testing
fn add_port(port: u16) -> SocketAddr {
    SocketAddr::from(([0, 0, 0, 0], port))
}
