use crate::cli;
use crate::config;
use crate::error;
use crate::error::Error;
use crate::prelude::*;
use crate::server::ok_if_exists;
use crate::transport;

use std::io::Write;
use std::net::SocketAddr;
use std::path::Path;

pub async fn pull(args: cli::Pull) -> Result<(), Error> {
    let config: config::Jobs<config::common::File> =
        config::load_config(&args.job_file).map_err(error::PullErrorLocal::from)?;

    let req = match args.filter {
        Some(cli::RegexFilter::Include { include }) => {
            validate_regex(&include)?;
            transport::PullFileRequest::new(
                include,
                true,
                config.namespace(),
                config.batch_name(),
                args.dry,
                args.skip_folders,
            )
        }
        Some(cli::RegexFilter::Exclude { exclude }) => {
            validate_regex(&exclude)?;
            transport::PullFileRequest::new(
                exclude,
                false,
                config.namespace(),
                config.batch_name(),
                args.dry,
                args.skip_folders,
            )
        }
        None => transport::PullFileRequest::new(
            vec![],
            false,
            config.namespace(),
            config.batch_name(),
            args.dry,
            args.skip_folders,
        ),
    };

    info!("connecing to server...");
    let addr = SocketAddr::from((args.ip, args.port));
    let mut conn = transport::Connection::new(addr).await?;
    debug!("finished connecting to server");

    if let Err(e) = conn.transport_data(&req.into()).await {
        error!(
            "error received from the server when sending pull request: {}",
            e
        );
    }

    if args.dry {
        match conn.receive_data().await? {
            transport::ServerResponseToUser::PullFilesDryResponse(resp) => {
                let stdout = std::io::stdout();
                let lock = stdout.lock();
                let mut writer = std::io::BufWriter::new(lock);

                writer.write_all(b"included files:").unwrap();
                for f in resp.success_files {
                    writer
                        .write_all(format!("\t{}\n", f.display()).as_bytes())
                        .unwrap();
                }

                writer.write_all(b"\nfiltered files:").unwrap();
                for f in resp.filtered_files {
                    writer
                        .write_all(format!("\t{}\n", f.display()).as_bytes())
                        .unwrap();
                }
            }
            transport::ServerResponseToUser::PullFilesError(e) => {
                error!("there was an error pulling files for the response: {}", e);
            }
            other => {
                error!("unexpected response to the dry query: {}", other);
                Err(error::PullErrorLocal::UnexpectedResponse)?;
            }
        }
    }
    // otherwise, if the request is not for a dry run we can expect
    // the server to start sending us some files
    else {
        if !args.save_dir.exists() {
            ok_if_exists(std::fs::create_dir(&args.save_dir))
                .map_err(|e| error::CreateDir::new(e, args.save_dir.to_owned()))
                .map_err(error::PullErrorLocal::from)?;
        }

        loop {
            match conn.receive_data().await? {
                transport::ServerResponseToUser::SendFile(file) => {
                    save_file(&args.save_dir, file)?;
                }
                transport::ServerResponseToUser::FileMarker(marker) => {
                    if let Err(e) = save_large_file(&mut conn, &args.save_dir, marker).await {
                        error!("error saving large file: {}", e);
                    }
                }
                transport::ServerResponseToUser::PullFilesError(e) => {
                    error!(
                        "there was an error on the server: {} - trying to continue",
                        e
                    );
                }
                transport::ServerResponseToUser::FinishFiles => break,
                other => {
                    error!(
                        "unexpected response from the server: {} - killing execution",
                        other
                    );
                    Err(error::PullErrorLocal::UnexpectedResponse)?;
                }
            }

            conn.transport_data(&transport::UserMessageToServer::FileReceived)
                .await?;
        }
    }

    Ok(())
}

/// helper function for collecting the TCP byte stream to a `Writer` instead of loading into memory
async fn save_large_file(
    conn: &mut transport::Connection<transport::UserMessageToServer>,
    base_path: &Path,
    marker: transport::FileMarker,
) -> Result<(), Box<dyn std::error::Error>> {
    let new_save_location = base_path.join(&marker.file_path);
    debug!(
        "saving large file from server at {} to {}",
        marker.file_path.display(),
        new_save_location.display()
    );

    // first, acknowledge that we have received the marker
    conn.transport_data(&transport::UserMessageToServer::FileReceived)
        .await?;

    // then, create a writer for this file to go to
    let file = tokio::fs::File::create(&new_save_location)
        .await
        .map_err(|e| error::WriteFile::new(e, new_save_location))?;
    let mut writer = file;

    // then, receive the raw byte stream directly to the writer
    conn.receive_to_writer(&mut writer).await?;

    writer.flush().await?;

    Ok(())
}

/// helper function to process a SendFile from the server and save it
/// to an appropriate location
fn save_file(save_location: &Path, file: transport::SendFile) -> Result<(), error::PullErrorLocal> {
    debug!(
        "path from the server that is being saved is {}",
        file.file_path.display()
    );

    let path = save_location.join(file.file_path);

    if file.is_file {
        ok_if_exists(std::fs::write(&path, file.bytes))
            .map_err(|e| error::WriteFile::new(e, path))?;
    } else {
        ok_if_exists(std::fs::create_dir(&path)).map_err(|e| error::CreateDir::new(e, path))?;
    }

    Ok(())
}

/// iterate through all user-provided regular expressions and make sure they all compile
/// correctly
fn validate_regex(exprs: &[String]) -> Result<(), error::PullErrorLocal> {
    for x in exprs {
        regex::Regex::new(x).map_err(|e| error::RegexError::new(x.to_string(), e))?;
    }

    Ok(())
}
