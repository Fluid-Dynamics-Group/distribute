use crate::error::{self, Error};
use crate::prelude::*;

pub async fn add(args: cli::Add) -> Result<(), Error> {
    //
    // load the config files
    //
    let jobs = config::load_config::<config::Jobs>(&args.jobs)?;

    if jobs.len_jobs() == 0 {
        return Err(Error::Add(error::AddError::NoJobsToAdd));
    }

    // ensure that there are no duplicate job names
    check_has_duplicates(&jobs.job_names())?;

    debug!("loading job information from files");
    let loaded_jobs = jobs.load_jobs().await.map_err(error::ServerError::from)?;

    debug!("loading build information from files");
    let loaded_build = jobs.load_build().await.map_err(error::ServerError::from)?;

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
        if cap.can_accept_job(jobs.capabilities()) {
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

    let job_set = server::OwnedJobSet::new(
        loaded_build,
        jobs.capabilities().clone(),
        loaded_jobs,
        jobs.batch_name(),
        jobs.matrix_user(),
        jobs.namespace(),
    );

    if !args.dry {
        debug!("sending job set to server");
        conn.transport_data(&transport::UserMessageToServer::AddJobSet(job_set))
            .await?;

        match conn.receive_data().await {
            Ok(transport::ServerResponseToUser::JobSetAdded) => (),
            Ok(transport::ServerResponseToUser::JobSetAddedFailed) => {
                Err(error::AddError::FailedToAdd)?
            }
            Ok(x) => return Err(Error::from(error::AddError::NotCapabilities(x))),
            Err(e) => Err(e)?,
        };
    } else {
        debug!("skipping message to the server for dry run");
    }

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
