#![allow(unused_imports)]

mod add;
mod cli;
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

use error::Error;

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
        .chain(fern::log_file("output.log").map_err(error::LogError::from)?)
        // Apply globally
        .apply()
        .map_err(error::LogError::from)?;

    match arguments {
        cli::Arguments::Client(client) => client::client_command(client).await,
        cli::Arguments::Server(server) => server::server_command(server).await,
        cli::Arguments::Status(status) => status::status_command(status).await,
        cli::Arguments::Kill(kill) => kill::kill(kill).await,
        cli::Arguments::Pause(pause) => pause::pause(pause).await,
        cli::Arguments::Add(add) => add::add(add).await,
        cli::Arguments::Template(template) => template::template(template),
        cli::Arguments::Pull(pull) => pull::pull(pull).await,
    }
}
