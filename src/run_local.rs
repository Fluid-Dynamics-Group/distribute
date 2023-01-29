use crate::cli;
use crate::client;
use crate::config;
use crate::error::RunErrorLocal;

use crate::transport;

use crate::prelude::*;

pub async fn run_local(args: cli::Run) -> Result<(), RunErrorLocal> {
    create_required_dirs(&args).await?;

    let (build, jobs) = load_config(&args.job_file).await?;

    let mut state = client::execute::BindingFolderState::new();

    let (_tx, mut rx) = tokio::sync::broadcast::channel(1);

    client::execute::initialize_apptainer_job(build, &args.save_dir, &mut state).await?;

    let archive = args.save_dir.join("archived_files");
    fs::create_dir(&archive)?;

    let distribute_save = args.save_dir.join("distribute_save");

    for job in jobs {
        let name = job.job_name.clone();
        client::execute::run_apptainer_job(job, &args.save_dir, &mut rx, &state).await?;
        fs::rename(&distribute_save, archive.join(name))?;
        fs::create_dir(&distribute_save)?;
    }

    Ok(())
}

async fn create_required_dirs(args: &cli::Run) -> Result<(), RunErrorLocal> {
    // make the directory structure that is required
    if args.save_dir.exists() {
        if args.clean_save {
            crate::client::utils::clean_output_dir(&args.save_dir).await?;
        } else {
            return Err(RunErrorLocal::FolderExists);
        }
    } else {
        crate::client::utils::clean_output_dir(&args.save_dir).await?;
    }

    Ok(())
}
