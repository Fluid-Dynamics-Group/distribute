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

async fn wrap_main() -> Result<(), Error> {
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
        .chain(fern::log_file("output.log").map_err(distribute::LogError::from)?)
        // Apply globally
        .apply()
        .map_err(distribute::LogError::from)?;

    match arguments {
        cli::Arguments::Client(client) => distribute::client_command(client).await,
        cli::Arguments::Server(server) => distribute::server_command(server).await,
        cli::Arguments::Status(status) => distribute::status_command(status).await,
        cli::Arguments::Kill(kill) => distribute::kill(kill).await,
        cli::Arguments::Pause(pause) => distribute::pause(pause).await,
        cli::Arguments::Add(add) => distribute::add(add).await,
        cli::Arguments::Template(template) => distribute::template(template),
        cli::Arguments::Pull(pull) => distribute::pull(pull).await,
    }
}
