use crate::cli;
use crate::client;
use crate::config;
use crate::error::RunErrorLocal;

use crate::prelude::*;

pub async fn run_local(args: cli::Run) -> Result<(), RunErrorLocal> {
    let working_dir = WorkingDir::from(args.save_dir.clone());

    create_required_dirs(&args, &working_dir).await?;

    let config = config::load_config::<config::Jobs<config::common::File>>(&args.job_file)?;

    let apptainer_config = match config {
        config::Jobs::Apptainer(app) => app,
        config::Jobs::Python(_) => return Err(RunErrorLocal::OnlyApptainer),
    };

    let mut state = client::execute::BindingFolderState::new();

    let (_tx, mut rx) = tokio::sync::broadcast::channel(1);

    working_dir
        .copy_initial_files_apptainer(&apptainer_config.description().initialize(), &mut state)
        .await;
    let archive = args.save_dir.join("archived_files");
    fs::create_dir(&archive)?;

    let distribute_save = args.save_dir.join("distribute_save");

    for job in apptainer_config.description().jobs().into_iter().cloned() {
        working_dir.copy_job_files_apptainer(&job).await;

        let job = job.hashed(0).unwrap();
        let name = job.name().to_string();

        client::execute::run_apptainer_job(job, &working_dir, &mut rx, &state).await?;
        fs::rename(&distribute_save, archive.join(name))?;
        fs::create_dir(&distribute_save)?;
    }

    Ok(())
}

async fn create_required_dirs(args: &cli::Run, workdir: &WorkingDir) -> Result<(), RunErrorLocal> {
    // make the directory structure that is required
    if args.save_dir.exists() {
        if args.clean_save {
            workdir.delete_and_create_folders().await?;
        } else {
            return Err(RunErrorLocal::FolderExists);
        }
    } else {
        workdir.delete_and_create_folders().await?;
    }

    Ok(())
}
