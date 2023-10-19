//! utilities for setting up logging
//!
//! access to this module is usually done through [`crate::logger`]. The `cli` feature must be
//! enabled for this module to be compiled

use crate::prelude::*;
use tracing::level_filters::LevelFilter;

/// create a logger instance sending output only to stdout
///
/// helper function to setup logging for some unit and integration tests. More
/// fine grained control of logging can be done with [`logger_cfg`]
pub fn default() {
    logger_cfg::<ThreadLocal>(LoggingOutput::Stdout, true);
}

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

/// generic constructor-trait for setting up either thread-local logging or global logging
///
/// To setup global logging, see [`Global`], and for thread-local logging see [`ThreadLocal`]
pub trait LogThreading {
    /// construct the logger from the provided [`tracing`] subscriber
    fn from_subscriber<S>(subscriber: S)
    where
        S: tracing::Subscriber + Send + Sync + 'static;
}

/// setup logger to be global between all threads
pub struct Global;

impl LogThreading for Global {
    fn from_subscriber<S>(subscriber: S)
    where
        S: tracing::Subscriber + Send + Sync + 'static,
    {
        tracing::subscriber::set_global_default(subscriber)
            .expect("setting default subscriber failed - have you already set a subscriber for the process globally?");
    }
}

/// setup logger to be local to each thread.
///
/// Leaks the lock guard that is returned such that it lives for the duration of
/// the program
struct ThreadLocal;

impl LogThreading for ThreadLocal {
    fn from_subscriber<S>(subscriber: S)
    where
        S: tracing::Subscriber + Send + Sync + 'static,
    {
        let guard = tracing::subscriber::set_default(subscriber);
        Box::leak(Box::new(guard));
    }
}

/// setup logging with a specific [`LoggingOutput`] configuration
///
/// ## Parameters
///
/// `with_filename`: enable filename in logs
pub fn logger_cfg<LOG>(logging_output: LoggingOutput, with_filename: bool)
where
    LOG: LogThreading,
{
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

            // initialize the logger to the given subscriber
            LOG::from_subscriber(subscriber)
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

            // initialize the logger to the given subscriber
            LOG::from_subscriber(subscriber)
        }
        LoggingOutput::File(file_writer) => {
            let subscriber = tracing_subscriber::FmtSubscriber::builder()
                .with_max_level(logging_level)
                .with_file(with_filename)
                .with_writer(file_writer)
                .with_ansi(false)
                .with_timer(time)
                .finish();

            // initialize the logger to the given subscriber
            LOG::from_subscriber(subscriber)
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

        logger_cfg::<ThreadLocal>(log_cfg, false);

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

        logger_cfg::<ThreadLocal>(log_cfg, false);

        helper_log_function(1, "1");
        helper_log_function(1, "2");
        helper_log_function(3, "3");
        helper_log_function(4, "4");

        std::fs::remove_file(path).unwrap();
    }

    #[instrument]
    fn helper_log_function(node_meta: usize, other_val: &str) {
        // TODO: its annoying this has to show up in logs, but it has to do with how output
        // is captured by `cargo test`
        // https://users.rust-lang.org/t/cargo-doesnt-capture-stderr-in-tests/67045
        error!("simulated error log for unit testing purposes")
    }
}
