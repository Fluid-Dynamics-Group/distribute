//! utilities for setting up logging
//!
//! access to this module is usually done through [`crate::logger`]. The `cli` feature must be
//! enabled for this module to be compiled

use tracing::level_filters::LevelFilter;
use crate::prelude::*;

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

impl LoggingOutput {
    /// generate a [`LevelFilter`] from [`LoggingOutput`]
    fn level(&self) -> LevelFilter {
        match self {
            Self::None => LevelFilter::OFF,
            _ => LevelFilter::DEBUG,
        }
    }
}

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
mod tests {
    use super::*;

    /// ensure that logs do not contain ANSI escape sequences when saved to files
    #[test]
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

    #[instrument]
    fn helper_log_function(node_meta: usize, other_val: &str) {
        error!("error in the helper log function! oh no! (this is simulated)")
    }
}
