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
    logger_cfg(LoggingOutput::Stdout, true);
}

#[cfg(feature = "cli")]
/// set locations for log outputs (stdout, file, both, or none)
pub enum LoggingOutput {
    /// log to stdout only
    Stdout,
    /// log to stdout *and* a file
    StdoutAndFile(fs::File),
    /// log exclusively a file
    File(fs::File),
    /// do not log
    None,
}

#[cfg(feature = "cli")]
impl LoggingOutput {
    /// generate a [`LevelFilter`] from [`LoggingOutput`]
    fn level(&self) -> LevelFilter {
        match self {
            Self::None => LevelFilter::OFF,
            _ => LevelFilter::DEBUG,
        }
    }
}

#[cfg(feature = "cli")]
/// setup logging with a specific [`LoggingOutput`] configuration
///
/// ## Parameters
///
/// `with_filename`: enable filename in logs
pub fn logger_cfg(logging_output: LoggingOutput, with_filename: bool) {
    use tracing_subscriber::fmt::time;

    let time = time::SystemTime;

    let stdout = std::io::stdout;
    let logging_level = tracing::Level::DEBUG;

    match logging_output {
        LoggingOutput::None => (),
        LoggingOutput::Stdout => {
            let subscriber = tracing_subscriber::FmtSubscriber::builder()
                .with_max_level(logging_level)
                .with_file(with_filename)
                .with_writer(stdout)
                .with_ansi(true)
                .with_timer(time)
                .finish();

            tracing::subscriber::set_global_default(subscriber)
                .expect("setting default subscriber failed");
        }
        LoggingOutput::StdoutAndFile(file_writer) => {
            use tracing_subscriber::{fmt, prelude::*, registry::Registry};

            let stdout_subscriber = fmt::Layer::new()
                .with_file(with_filename)
                .with_writer(stdout.with_max_level(logging_level))
                .with_timer(time.clone())
                .with_ansi(false);

            let file_subscriber = fmt::Layer::new()
                .with_file(with_filename)
                .with_writer(file_writer.with_max_level(logging_level))
                .with_timer(time)
                .with_ansi(false);

            let subscriber = Registry::default()
                .with(file_subscriber)
                .with(stdout_subscriber);

            tracing::subscriber::set_global_default(subscriber)
                .expect("setting default subscriber failed");
        }
        LoggingOutput::File(file_writer) => {
            let subscriber = tracing_subscriber::FmtSubscriber::builder()
                .with_max_level(logging_level)
                .with_file(with_filename)
                .with_writer(file_writer)
                .with_ansi(false)
                .with_timer(time)
                .finish();

            tracing::subscriber::set_global_default(subscriber)
                .expect("setting default subscriber failed");
        }
    }
}

#[cfg(test)]
/// get a local address at a given port. Used exclusively for testing
fn add_port(port: u16) -> SocketAddr {
    SocketAddr::from(([0, 0, 0, 0], port))
}

#[cfg(test)]
/// ensure that logs do not contain ANSI escape sequences when saved to files
//#[test]
fn logs_no_ansi() {
    let path = "./no_ansi_logs.txt";
    let file = std::fs::File::create(path).unwrap();
    let log_cfg = LoggingOutput::File(file);

    logger_cfg(log_cfg, false);

    helper_log_function(1, "1");
    helper_log_function(1, "2");
    helper_log_function(3, "3");
    helper_log_function(4, "4");

    std::fs::remove_file(path).unwrap();
}

#[cfg(test)]
/// ensure that logs do not contain ANSI escape sequences when saved to files
/// while also being written to stdout
#[test]
fn logs_no_ansi_with_stdout() {
    let path = "./no_ansi_logs_and_stdout.txt";
    let file = std::fs::File::create(path).unwrap();
    let log_cfg = LoggingOutput::StdoutAndFile(file);

    logger_cfg(log_cfg, false);

    helper_log_function(1, "1");
    helper_log_function(1, "2");
    helper_log_function(3, "3");
    helper_log_function(4, "4");

    std::fs::remove_file(path).unwrap();
}

#[cfg(test)]
#[instrument]
fn helper_log_function(node_meta: usize, other_val: &str) {
    error!("error in the helper log function! oh no! (this is simulated)")
}
