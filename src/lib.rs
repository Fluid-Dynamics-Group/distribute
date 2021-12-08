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
mod server;
mod status;
mod template;
mod transport;
mod run_local;

pub use error::Error;
pub use error::RunErrorLocal;
pub use error::LogError;

#[macro_use]
extern crate log;

pub use config::*;
pub use server::*;

pub use client::client_command;
pub use server::server_command;
pub use status::status_command;
pub use status::get_current_jobs;
pub use kill::kill;
pub use pause::pause;
pub use add::add;
pub use template::template;
pub use pull::pull;
pub use run_local::run_local;


#[cfg(test)]
mod reexports {
    use super::cli;
    use super::server;
    use super::pull;

    pub async fn start_server(args: cli::Server) -> Result<(), Box<dyn std::error::Error>> {
        server::server_command(args).await?;
        Ok(())
    }

    pub async fn start_pull(args: cli::Pull) -> Result<(), Box<dyn std::error::Error>> {
        pull::pull(args).await?;
        Ok(())
    }

}
