use distribute::cli;
use distribute::Error;
use std::fs;

use clap::Parser;

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() {
    if let Err(e) = wrap_main().await {
        println!("{}", e);
    }
}

async fn wrap_main() -> Result<(), ErrorWrap> {
    let arguments = cli::ArgsWrapper::parse();

    setup_logs(&arguments)?;

    match arguments.command {
        cli::Arguments::Client(client) => distribute::client_command(client)
            .await
            .map_err(ErrorWrap::from),
        cli::Arguments::Server(server) => distribute::server_command(server)
            .await
            .map_err(ErrorWrap::from),
        cli::Arguments::Kill(kill) => distribute::kill(kill).await.map_err(ErrorWrap::from),
        cli::Arguments::Pause(pause) => distribute::pause(pause).await.map_err(ErrorWrap::from),
        cli::Arguments::Add(add) => distribute::add(add).await.map_err(ErrorWrap::from),
        cli::Arguments::Template(template) => {
            distribute::template(template).map_err(ErrorWrap::from)
        }
        cli::Arguments::Pull(pull) => distribute::pull(pull).await.map_err(ErrorWrap::from),
        cli::Arguments::Run(run) => distribute::run_local(run).await.map_err(ErrorWrap::from),
        cli::Arguments::ServerStatus(s) => {
            distribute::server_status(s).await.map_err(ErrorWrap::from)
        }
        cli::Arguments::NodeStatus(s) => distribute::node_status(s).await.map_err(ErrorWrap::from),
    }
}

fn setup_logs(args: &cli::ArgsWrapper) -> Result<(), ErrorWrap> {
    let logging_output = match (args.save_log, args.show_logs) {
        // saving a log, and creating a log file
        (true, true) => {
            let file = fs::File::create(args.command.log_path())
                .map_err(|e| distribute::CreateFile::new(e, args.command.log_path()))
                .map_err(distribute::LogError::from)
                .map_err(distribute::Error::from)?;
            distribute::LoggingOutput::StdoutAndFile(file)
        }
        // saving a log, not showing the logs
        (true, false) => {
            let file = fs::File::create(args.command.log_path())
                .map_err(|e| distribute::CreateFile::new(e, args.command.log_path()))
                .map_err(distribute::LogError::from)
                .map_err(distribute::Error::from)?;
            distribute::LoggingOutput::File(file)
        }
        // not saving a log, showing a log
        (false, true) => distribute::LoggingOutput::Stdout,
        // not saving a log, not showing a log
        (false, false) => distribute::LoggingOutput::None,
    };

    distribute::logger_cfg(logging_output, false);

    Ok(())
}

#[derive(derive_more::From, thiserror::Error, Debug)]
enum ErrorWrap {
    #[error("{0}")]
    Error(Error),
    #[error("{0}")]
    RunLocal(distribute::RunErrorLocal),
}
