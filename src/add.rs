use crate::error::{self, Error};
use crate::prelude::*;
use sha1::Digest;

use crate::client::execute::FileMetadata;

pub async fn add(args: cli::Add) -> Result<(), Error> {
    //
    // load the config files
    //
    let mut jobs = config::load_config::<config::Jobs>(&args.jobs)?;

    if jobs.len_jobs() == 0 {
        return Err(Error::Add(error::AddError::NoJobsToAdd));
    }

    // ensure that there are no duplicate job names
    check_has_duplicates(&jobs.job_names())?;

    debug!("loading job information from files");
    let loaded_jobs = jobs.jobset_files().await.map_err(error::ServerError::from)?;

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
    let mut hashed_files_to_send: Vec<FileMetadata> = Vec::new();

    match &mut jobs {
        config::Jobs::Apptainer(app) => {
            let description = app.description_mut();
            // first, send the leading sif file
            let init_hash = hashing::filename_hash(&description.initialize);

            let path_string =  format!("setup_sif_{init_hash}.dist");
            let path_buf = PathBuf::from(path_string);

            let sif_meta = FileMetadata { 
                absolute_file_path: description.initialize.sif.to_owned(),
                relative_file_path: path_buf.clone(),
                is_file: true
            };
            description.initialize.sif = path_buf.clone();

            hashed_files_to_send.push(sif_meta);

            // add the init required files
            for (idx, init_file) in description.initialize.required_files.iter_mut().enumerate() {
                let path_string = format!("{init_hash}_{idx}.dist");
                let meta = FileMetadata {
                    absolute_file_path: init_file.path().to_owned(),
                    relative_file_path: path_string.into(),
                    is_file: true,
                };
                hashed_files_to_send.push(meta);
            }

            // then for every job, also send the files they require
            for (job_idx, job) in description.jobs.iter_mut().enumerate() {
                let hash = hashing::filename_hash(job);
                for (file_idx, file) in job.required_files.iter_mut().enumerate() {
                    let relative_file_path = format!("{init_hash}_{job_idx}_{file_idx}.dist").into();

                    let meta = FileMetadata {
                        absolute_file_path: file.path().to_owned(),
                        relative_file_path,
                        is_file: true,
                    };

                    hashed_files_to_send.push(meta);

                }
            }

        }
        config::Jobs::Python(py) => todo!()
    }


    if args.dry {
        debug!("skipping message to the server for dry run");
        return Ok(())
    }

    debug!("sending job set to server");
    conn.transport_data(&transport::UserMessageToServer::AddJobSet(jobs))
        .await?;

    // wait for the notice that we can continue 
    debug!("awaiting notice from the server that we can continue with sending the files");
    conn.receive_data().await?;

    //
    // send all the files with send_files state machin
    //
    let conn = conn.update_state();

    let extra = protocol::send_files::FlatFileList { files: hashed_files_to_send };
    let state = protocol::send_files::SenderState { conn, extra };
    let machine = protocol::Machine::from_state(state);

    let mut conn : transport::Connection<transport::UserMessageToServer> = match machine.send_files().await {
        Ok(next) => next.into_inner().update_state(),
        Err((_machine, e)) =>  {
            error!("failed to send_files to server, error: {e}");
            return Err(Error::from(error::AddError::FailedSend))
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

mod hashing {
    use super::*;

    trait HashableComponent {
        /// either python compilation file, or the singularity .sif file
        fn job_file(&self) -> PathBuf;
        /// name of the job
        fn job_name(&self) -> &str;
        /// all the files associated with this component
        fn files(&self) -> Box<dyn Iterator<Item=PathBuf>>;
    }

    impl HashableComponent for config::apptainer::Initialize {
        fn job_file(&self) -> PathBuf {
            self.sif.to_owned()
      }
        fn job_name(&self) -> &str {
            "initialize"
        }
        fn files(&self) -> Box<dyn Iterator<Item=PathBuf>> {
            let iter = self.required_files
                .into_iter()
                .map(|f| f.path().to_owned());

            Box::new(iter)
        }
    }

    impl HashableComponent for config::python::Initialize {
        fn job_file(&self) -> PathBuf {
            self.python_build_file_path().to_owned()
      }
        fn job_name(&self) -> &str {
            "initialize"
        }
        fn files(&self) -> Box<dyn Iterator<Item=PathBuf>> {
            let iter = self.required_files()
                .into_iter()
                .map(|f| f.path().to_owned());

            Box::new(iter)
        }
    }

    impl HashableComponent for config::apptainer::Job {
        fn job_file(&self) -> PathBuf {
            PathBuf::from("job")
        }
        fn job_name(&self) -> &str {
            self.name()
        }
        fn files(&self) -> Box<dyn Iterator<Item=PathBuf>> {
            let iter = self.required_files()
                .into_iter()
                .map(|f| f.path().to_owned());

            Box::new(iter)
        }
    }

    impl HashableComponent for config::python::Job {
        fn job_file(&self) -> PathBuf {
            self.python_job_file().to_owned()
        }
        fn job_name(&self) -> &str {
            self.name()
        }
        fn files(&self) -> Box<dyn Iterator<Item=PathBuf>> {
            let iter = self.required_files()
                .into_iter()
                .map(|f| f.path().to_owned());

            Box::new(iter)
        }
    }

    pub(super) fn filename_hash<T: HashableComponent>(data: &T) -> String {
        let mut sha = sha1::Sha1::new();

        sha.update(data.job_name().as_bytes());

        for file in data.files() {
            let filename = file.file_name().unwrap().to_string_lossy();
            sha.update(filename.as_bytes());
            let bytes = std::fs::read(file).unwrap();
            sha.update(&bytes);
        }

        let hash = base16::encode_lower(&sha.finalize());

        hash
    }
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
