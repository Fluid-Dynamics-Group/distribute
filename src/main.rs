#![allow(unused_imports)]

mod add;
mod cli;
mod client;
mod config;
mod error;
mod pause;
mod server;
mod status;
mod transport;
mod template;

use error::Error;

#[macro_use]
extern crate log;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    if let Err(e) = wrap_main().await {
        println!("{}", e);
    }
}

async fn wrap_main() -> Result<(), Error> {
    let arguments: cli::Arguments = argh::from_env();

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

    match arguments.command {
        cli::Command::Client(client) => client::client_command(client).await,
        cli::Command::Server(server) => server::server_command(server).await,
        cli::Command::Status(status) => status::status_command(status).await,
        cli::Command::Pause(pause) => pause::pause(pause).await,
        cli::Command::Add(add) => add::add(add).await,
        cli::Command::Template(template) => template::template(template)
    }
}
