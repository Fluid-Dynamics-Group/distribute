use crate::cli;
use crate::client;
use crate::config;
use crate::error::{self, Error, RunErrorLocal};
use crate::server;
use crate::transport;

use std::fs;
use std::path::{Path, PathBuf};

pub async fn run_local(args: cli::Run) -> Result<(), RunErrorLocal> {

    create_required_dirs(&args).await?;

    let (build, jobs) = load_config(&args.job_file).await?;

    let mut state = client::execute::BindingFolderState::new();

    let (_tx, mut rx) = tokio::sync::broadcast::channel(1);

    client::execute::initialize_singularity_job(build, &args.save_dir, &mut rx, &mut state).await?;

    for job in jobs {
        client::execute::run_singularity_job(job, &args.save_dir, &mut rx, &mut state).await?; 
    }
    
    Ok(())
}

async fn create_required_dirs(args: &cli::Run) -> Result<(), RunErrorLocal> {
    // make the directory structure that is required
    if args.save_dir.exists() {
        if args.clean_save {
            crate::client::utils::clean_output_dir(&args.save_dir).await?;
        }
        else {
            return Err(RunErrorLocal::FolderExists) 
        }
    } else {
        crate::client::utils::clean_output_dir(&args.save_dir).await?;
    }

    Ok(())
}

/// load the config files 
async fn load_config(path: &Path) -> Result<(transport::SingularityJobInit, Vec<transport::SingularityJob>), RunErrorLocal> {
    let jobs = config::load_config::<config::Jobs>(&path.to_string_lossy())?;

    debug!("loading job information from files");
    let loaded_jobs = match jobs.load_jobs().await? {
        config::JobOpts::Python(_) => {
            return Err(RunErrorLocal::OnlyApptainer)
        },
        config::JobOpts::Singularity(s) => s
    };

    debug!("loading build information from files");
    let loaded_build = match jobs.load_build().await? {
        config::BuildOpts::Python(_) => {
            return Err(RunErrorLocal::OnlyApptainer)
        },
        config::BuildOpts::Singularity(s) => s
    };

    Ok((loaded_build, loaded_jobs))
}
