#![allow(unused_imports)]
#![allow(dead_code)]

mod add;
pub mod cli;
mod client;
mod config;
mod error;
mod kill;
mod pause;
mod pull;
mod run_local;
mod server;
mod status;
mod template;
mod transport;

pub use error::Error;
pub use error::LogError;
pub use error::RunErrorLocal;

#[macro_use]
extern crate log;

pub use config::*;
pub use server::*;

pub use add::add;
pub use client::client_command;
pub use kill::kill;
pub use pause::pause;
pub use pull::pull;
pub use run_local::run_local;
pub use server::server_command;
pub use status::get_current_jobs;
pub use status::status_command;
pub use template::template;

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
