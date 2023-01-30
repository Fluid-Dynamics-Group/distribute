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

#[cfg(feature = "cli")]
use prelude::*;

#[cfg(feature = "cli")]
pub use error::{CreateFile, Error, LogError, RunErrorLocal};

#[macro_use]
#[cfg(feature = "cli")]
extern crate tracing;

#[cfg(feature = "cli")]
use tracing::level_filters::LevelFilter;

pub use config::*;

pub use serde_yaml;

pub use matrix_notify::{OwnedUserId, UserId};

#[cfg(feature = "cli")]
pub use {
    add::add,
    client::client_command,
    kill::kill,
    node_status::node_status,
    pause::pause,
    pull::pull,
    run_local::run_local,
    server::server_command,
    server::RemainingJobs,
    server_status::get_current_jobs,
    server_status::server_status,
    template::template,
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
    logger_cfg(LoggingOutput::Stdout, true);
}

#[cfg(feature = "cli")]
pub enum LoggingOutput {
    Stdout,
    StdoutAndFile(fs::File),
    File(fs::File),
    None,
}

#[cfg(feature = "cli")]
impl LoggingOutput {
    fn level(&self) -> LevelFilter {
        match self {
            Self::None => LevelFilter::OFF,
            _ => LevelFilter::DEBUG,
        }
    }
}

// helper macro to create the subscriber since each individual `$writer` is a distinct type,
// and they are difficult / impossible to express as boxed trait objects
#[cfg(feature = "cli")]
macro_rules! subscriber_helper {
    ($writer:expr, $with_filename:expr, $level:expr) => {
        let subscriber = tracing_subscriber::FmtSubscriber::builder()
            .with_max_level($level)
            .with_file($with_filename)
            .with_writer($writer)
            .finish();

        tracing::subscriber::set_global_default(subscriber)
            .expect("setting default subscriber failed");
    };
}

#[cfg(feature = "cli")]
pub fn logger_cfg(logging_output: LoggingOutput, with_filename: bool) {
    let stdout = std::io::stdout;
    let logging_level = logging_output.level();

    match logging_output {
        LoggingOutput::None | LoggingOutput::Stdout => {
            let writer = stdout;

            // if the logging output is none, then the logging_level will be set to
            // off, and this case will handle itself
            subscriber_helper!(writer, with_filename, logging_level);
        }
        LoggingOutput::StdoutAndFile(file_writer) => {
            let writer = tracing_subscriber::fmt::writer::Tee::new(file_writer, stdout);
            subscriber_helper!(writer, with_filename, logging_level);
        }
        LoggingOutput::File(file_writer) => {
            let writer = file_writer;
            subscriber_helper!(writer, with_filename, logging_level);
        }
    }
}

#[cfg(test)]
fn add_port(port: u16) -> SocketAddr {
    SocketAddr::from(([0, 0, 0, 0], port))
}
