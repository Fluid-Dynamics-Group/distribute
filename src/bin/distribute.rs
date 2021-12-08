#![allow(unused_imports)]


use distribute::Error;
use distribute::cli;

use structopt::StructOpt;

#[macro_use]
extern crate log;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    if let Err(e) = wrap_main().await {
        println!("{}", e);
    }
}

async fn wrap_main() -> Result<(), ErrorWrap> {
    let arguments = cli::Arguments::from_args();

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
        .level(log::LevelFilter::Debug)
        // - and per-module overrides
        .level_for("hyper", log::LevelFilter::Info)
        // Output to stdout, files, and other Dispatch configurations
        .chain(std::io::stdout())
        .chain(fern::log_file(&arguments.log_path()).map_err(distribute::LogError::from).map_err(Error::from)?)
        // Apply globally
        .apply()
        .map_err(distribute::LogError::from)
        .map_err(Error::from)?;

    match arguments {
        cli::Arguments::Client(client) => distribute::client_command(client).await.map_err(ErrorWrap::from),
        cli::Arguments::Server(server) => distribute::server_command(server).await.map_err(ErrorWrap::from),
        cli::Arguments::Status(status) => distribute::status_command(status).await.map_err(ErrorWrap::from),
        cli::Arguments::Kill(kill) => distribute::kill(kill).await.map_err(ErrorWrap::from),
        cli::Arguments::Pause(pause) => distribute::pause(pause).await.map_err(ErrorWrap::from),
        cli::Arguments::Add(add) => distribute::add(add).await.map_err(ErrorWrap::from),
        cli::Arguments::Template(template) => distribute::template(template).map_err(ErrorWrap::from),
        cli::Arguments::Pull(pull) => distribute::pull(pull).await.map_err(ErrorWrap::from),
        cli::Arguments::Run(pull) => distribute::run_local(pull).await.map_err(ErrorWrap::from),
    }
}

#[derive(derive_more::From, thiserror::Error, Debug)]
enum ErrorWrap {
    #[error("{0}")]
    Error(Error),
    #[error("{0}")]
    RunLocal(distribute::RunErrorLocal)
}
