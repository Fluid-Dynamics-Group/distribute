use crate::error::{self, Error};
use crate::prelude::*;

pub async fn add(args: cli::Add) -> Result<(), Error> {
    //
    // load the config files
    //
    let config = config::load_config::<config::Jobs<config::common::File>>(&args.jobs)?;

    if config.len_jobs() == 0 {
        return Err(Error::Add(error::AddError::NoJobsToAdd));
    }

    // ensure that there are no duplicate job names
    check_has_duplicates(&config.job_names())?;

    debug!("ensuring all files exist on disk in the locations described");

    config.verify_config().map_err(error::AddError::from)?;

    //
    // check the server for all of the node capabilities
    //

    let addr = SocketAddr::from((args.ip, args.port));

    let mut conn = transport::Connection::new(addr).await?;

    conn.transport_data(&transport::UserMessageToServer::QueryCapabilities)
        .await?;

    let caps = match conn.receive_data().await {
        Ok(transport::ServerResponseToUser::Capabilities(x)) => x,
        Ok(x) => return Err(Error::from(error::AddError::NotCapabilities(x))),
        Err(e) => Err(e)?,
    };

    if args.show_caps {
        println!("all node capabilities:");

        for cap in &caps {
            println!("{}", cap);
        }
    }

    //
    // calculate how many of the nodes can run this command
    //

    let total_nodes = caps.len();
    let mut working_nodes = 0;

    for cap in &caps {
        if cap.can_accept_job(config.capabilities()) {
            working_nodes += 1;
        }
    }

    println!(
        "these jobs can be run on {}/{} of the nodes",
        working_nodes, total_nodes
    );

    if working_nodes == 0 {
        return Err(error::AddError::NoCompatableNodes)?;
    }

    //
    // construct the job set and send it off
    //

    let hashed_config = config.hashed().map_err(error::AddError::from)?;

    dbg!(&hashed_config);

    let hashed_files_to_send = hashed_config.sendable_files(true);

    if args.dry {
        debug!("skipping message to the server for dry run");
        return Ok(());
    }

    let sendable_config = hashed_config.into();

    debug!("sending job set to server");
    conn.transport_data(&transport::UserMessageToServer::AddJobSet(sendable_config))
        .await?;

    // wait for the notice that we can continue
    debug!("awaiting notice from the server that we can continue with sending the files");
    conn.receive_data().await?;

    //
    // send all the files with send_files state machine
    //
    let conn = conn.update_state();

    let extra = protocol::send_files::FlatFileList {
        files: hashed_files_to_send,
    };
    let state = protocol::send_files::SenderState { conn, extra };
    let machine = protocol::Machine::from_state(state);

    let mut conn: transport::Connection<transport::UserMessageToServer> =
        match machine.send_files().await {
            Ok(next) => next.into_inner().update_state(),
            Err((_machine, e)) => {
                error!("failed to send_files to server, error: {e}");
                return Err(Error::from(error::AddError::FailedSend));
            }
        };

    match conn.receive_data().await {
        Ok(transport::ServerResponseToUser::JobSetAdded) => (),
        Ok(transport::ServerResponseToUser::JobSetAddedFailed) => {
            Err(error::AddError::FailedToAdd)?
        }
        Ok(x) => return Err(Error::from(error::AddError::NotCapabilities(x))),
        Err(e) => Err(e)?,
    };

    Ok(())
}

fn check_has_duplicates<T: Eq + std::fmt::Display>(list: &[T]) -> Result<(), error::AddError> {
    for i in list {
        let mut count = 0;
        for j in list {
            if i == j {
                count += 1;
            }
        }

        if count > 1 {
            return Err(error::AddError::DuplicateJobName(i.to_string()));
        }
    }

    Ok(())
}

#[test]
fn pass_no_duplicate_entires() {
    let list = [1, 2, 3, 4, 5, 6];

    assert_eq!(check_has_duplicates(list.as_slice()).is_ok(), true);
}

#[test]
fn fail_no_duplicate_entires() {
    let list = [1, 1, 2, 3, 4, 5, 6];

    assert_eq!(check_has_duplicates(list.as_slice()).is_ok(), false);
}
