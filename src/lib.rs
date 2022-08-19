#![allow(dead_code)]

mod config;

#[cfg(feature = "cli")]
mod add;
#[cfg(feature = "cli")]
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
mod template;
#[cfg(feature = "cli")]
mod transport;

#[cfg(test)]
use prelude::*;

#[cfg(feature = "cli")]
pub use error::{Error, LogError, RunErrorLocal};

#[macro_use]
#[cfg(feature = "cli")]
extern crate log;

pub use config::*;
pub use matrix_notify::UserId;
pub use serde_yaml;

#[cfg(feature = "cli")]
pub use {
    add::add, client::client_command, kill::kill, node_status::node_status, pause::pause,
    pull::pull, run_local::run_local, server::server_command, server::RemainingJobs,
    server_status::get_current_jobs, server_status::server_status, template::template,
};

#[cfg(test)]
mod reexports {
    use super::cli;
    use super::pull;
    use super::server;

    pub async fn start_server(args: cli::Server) -> Result<(), Box<dyn std::error::Error>> {
        server::server_command(args).await?;
        Ok(())
    }

    pub async fn start_pull(args: cli::Pull) -> Result<(), Box<dyn std::error::Error>> {
        pull::pull(args).await?;
        Ok(())
    }
}

// helper function to setup logging in some integration tests
#[cfg(feature = "cli")]
pub fn logger() {
    fern::Dispatch::new()
        // Perform allocation-free log formatting
        .format(|out, message, record| {
            out.finish(format_args!(
                "{}[{}][{}] {}",
                chrono::Local::now().format("[%Y-%m-%d][%H:%M:%S]"),
                record.target(),
                record.level(),
                message
            ))
        })
        // Add blanket level filter -
        .level(log::LevelFilter::Trace)
        // - and per-module overrides
        .level_for("hyper", log::LevelFilter::Info)
        //.level_for("distribute::transport", log::LevelFilter::Debug)
        .level_for("mio", log::LevelFilter::Info)
        // Output to stdout, files, and other Dispatch configurations
        .chain(std::io::stdout())
        // Apply globally
        .apply()
        .map_err(LogError::from)
        .unwrap();
}

#[cfg(test)]
fn add_port(port: u16) -> SocketAddr {
    SocketAddr::from(([0, 0, 0, 0], port))
}
