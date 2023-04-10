use crate::prelude::*;
use crate::server::ok_if_exists;

const SIF_FILE: &str = "apptainer.sif";
const CURRENT_DUR_BASH: &[u8] = br#"DIR="$(cd "$(dirname "$0")" && pwd)""#;

/// convert a distribute job to a slurm job
pub fn slurm(args: cli::Slurm) -> Result<(), error::Slurm> {
    //
    // load the config files
    //
    let jobs = config::load_config::<config::Jobs<config::common::File>>(&args.jobs, true)?;

    if jobs.len_jobs() == 0 {
        return Err(error::Slurm::NoJobs);
    }

    // ensure that there are no duplicate job names
    crate::add::check_has_duplicates(&jobs.job_names())?;

    let apptainer_config = match jobs {
        config::Jobs::Python(_) => return Err(error::Slurm::PythonConfig),
        config::Jobs::Apptainer(apptainer) => apptainer,
    };

    let global_slurm = apptainer_config.slurm();

    let description = apptainer_config.description();
    let initialize = description.initialize();

    let sif_path = initialize.sif().path();
    let overall_files = initialize.required_files();
    let required_mounts = initialize.required_mounts();

    // get a list of mounts for this folder
    let mounts: Vec<Mount> = required_mounts
        .iter()
        .map(PathBuf::to_owned)
        .enumerate()
        .map(|(idx, container_path)| {
            let host_fs_folder_name = format!("mnt_{idx:02}");
            Mount {
                container_path,
                host_fs_folder_name,
            }
        })
        .collect();

    let jobs = description.jobs();

    // create the output directory
    create_dir_ok_if_exists(&args.output_folder)?;

    //
    // Input files
    //

    let mut global_input_files: Vec<InputFile> = Vec::with_capacity(overall_files.len());

    // create the input directory within the output folder that will store
    // symlinks to the files that should be moved to the server
    let overall_input_files = args.output_folder.join("input");
    create_dir_ok_if_exists(&overall_input_files)?;

    // copy the input files to this directory, using the alias that was stored in the config file
    for file in overall_files {
        let filename_or_alias = file.filename()?;
        let destination = overall_input_files.join(&filename_or_alias);

        copy(file.path(), &destination)?;

        let input = InputFile::new(destination, filename_or_alias);
        global_input_files.push(input);
    }

    // copy the input SIF file to the output folder (no symlink)
    let stored_sif = args.output_folder.join(SIF_FILE);
    copy(sif_path, stored_sif)?;

    //
    // Jobs
    //

    let mut job_names = Vec::new();

    for job in jobs {
        let slurm_config =
            determine_slurm_configuration(job.name(), global_slurm.clone(), job.slurm().clone())?;

        info!(distribtue_job_name = job.name(), slurm = ?slurm_config, "slurm job configuration output generated");

        //dbg!(&slurm_config);

        let job_name = slurm_config.job_name().as_ref().unwrap();
        let ntasks = slurm_config.ntasks().as_ref().unwrap();

        job_names.push(job_name.to_string());

        let job_folder = args.output_folder.join(job_name);
        let job_input_folder = job_folder.join("input");
        let job_output_folder = job_folder.join("output");

        create_dir_ok_if_exists(&job_folder)?;
        create_dir_ok_if_exists(&job_input_folder)?;
        create_dir_ok_if_exists(job_output_folder)?;

        create_mounts_at_basepath(&job_folder, &mounts)?;

        //
        // Batch file handling
        //
        let slurm_batch_file = job_folder.join("slurm_input.sl");
        let mut file = std::fs::File::create(&slurm_batch_file)
            .map_err(|e| error::CreateFile::new(e, slurm_batch_file.clone()))?;

        slurm_header(&mut file).map_err(|e| error::WriteFile::new(e, slurm_batch_file.clone()))?;

        slurm_config
            .write_slurm_config(&mut file)
            .map_err(|e| error::WriteFile::new(e, slurm_batch_file.clone()))?;

        slurm_footer(&mut file, mounts.as_slice(), *ntasks)
            .map_err(|e| error::WriteFile::new(e, slurm_batch_file.clone()))?;

        //
        // copy the input files for this job to the correct directory
        //
        for file in job.required_files() {
            let filename_or_alias = file.filename()?;
            let destination = job_input_folder.join(filename_or_alias);
            copy(file.path(), destination)?;
        }

        // now, symlink all the input files `overall_files` to the input directory
        // for this particular
        for file in global_input_files.iter() {
            // folder structure is
            //  output_folder
            //      - input
            //          - input_file_1
            //          - input_file_2
            //      - job_folder
            //          - input
            //              - input_file_1
            //              - input_file_2
            //              - input_file_3
            //
            // from job_folder/input/ we know that
            //  ../ gets to job_folder
            //  ../../ gets to output_folder
            //  ../../input gets to output_folder/input

            // the location that the symlink will point to
            let original_file = PathBuf::from(format!("../../input/{}", file.filename));
            // the location where we place the symlink on the filesystem
            let place_symlink_at = job_input_folder.join(&file.filename);

            std::os::unix::fs::symlink(original_file, place_symlink_at)
                .expect("failed to create symlink, this should not happen");
        }
    }

    rsync_upload_command(&args)?;
    schedule_jobs_sciprt(&args, &job_names)?;

    Ok(())
}

fn copy<T: AsRef<Path>, U: AsRef<Path>>(src: T, dest: U) -> Result<(), error::CopyFile> {
    let src = src.as_ref();
    let dest = dest.as_ref();

    std::fs::copy(src, dest)
        .map_err(|e| error::CopyFile::new(e, src.to_owned(), dest.to_owned()))?;

    Ok(())
}

fn create_mounts_at_basepath(basepath: &Path, mounts: &[Mount]) -> Result<(), error::CreateDir> {
    for mount in mounts {
        let folder_path = basepath.join(&mount.host_fs_folder_name);
        create_dir_ok_if_exists(folder_path)?;
    }

    Ok(())
}

fn create_dir_ok_if_exists<T: AsRef<Path>>(dir: T) -> Result<(), error::CreateDir> {
    ok_if_exists(fs::create_dir(dir.as_ref()))
        .map_err(|e| error::CreateDir::new(e, dir.as_ref().to_owned()))?;

    Ok(())
}

#[derive(Debug, Constructor)]
struct Mount {
    container_path: PathBuf,
    host_fs_folder_name: String,
}

#[derive(Debug, Constructor)]
struct InputFile {
    path: PathBuf,
    filename: String,
}

#[instrument]
fn determine_slurm_configuration(
    job_name: &str,
    batch_slurm_config: Option<config::Slurm>,
    // configuration for an individual job in the batch of jobs
    job_slurm_config: Option<config::Slurm>,
) -> Result<config::Slurm, error::SlurmInformation> {
    debug!("building slurm configuration for job");

    match (batch_slurm_config, job_slurm_config) {
        (Some(batch), Some(mut job)) => {
            // default the job_name slurm field to the name of the job that we have in the config
            // file
            job.set_default_job_name(job_name);
            //dbg!(&job);
            Ok(batch.override_with(&job))
        }
        (Some(mut batch), None) => {
            // default the job_name slurm field to the name of the job that we have in the config
            // file
            batch.set_default_job_name(job_name);
            Ok(batch)
        }
        (None, Some(mut job)) => {
            // default the job_name slurm field to the name of the job that we have in the config
            // file
            job.set_default_job_name(job_name);
            Ok(job)
        }
        (None, None) => Err(error::SlurmInformation::new(job_name.into())),
    }
}

fn slurm_header<W: Write>(mut writer: W) -> Result<(), std::io::Error> {
    writeln!(&mut writer, "#! /bin/bash\n")?;

    Ok(())
}

fn slurm_footer<W: Write>(
    mut writer: W,
    mounts: &[Mount],
    tasks: usize,
) -> Result<(), std::io::Error> {
    writeln!(&mut writer, "\nmodule load singularity")?;
    write!(&mut writer, "\nsingularity run --nv --app distribute")?;

    if !mounts.is_empty() {
        write!(&mut writer, " --bind ")?;
    }

    let input_arr = [
        Mount::new(PathBuf::from("/input"), "$PWD/input".to_string()),
        Mount::new(PathBuf::from("/distribute_save"), "$PWD/output".to_string()),
    ];

    let mut mounts_iter = input_arr.iter().chain(mounts.iter()).peekable();

    // have to use a loop {} here so that we can take advantage of peekable iterators
    while let Some(mount) = mounts_iter.next() {
        // TODO: may need to make this host_fs path relative to the root folder instead of just the
        // name, since purely the name of the folder may make this job unable to launch from
        // outside this directory...
        write!(
            &mut writer,
            "{}:{}",
            mount.host_fs_folder_name,
            mount.container_path.display()
        )?;

        // if we are not the last item in the iterator, then add a comma for the next item
        if mounts_iter.peek().is_some() {
            write!(&mut writer, ",")?;
        }
    }

    write!(&mut writer, " ../{SIF_FILE} {tasks}")?;

    Ok(())
}

/// write a file that the user can use to upload the files to the cluster
fn rsync_upload_command(args: &cli::Slurm) -> Result<(), error::Slurm> {
    let filename = args.output_folder.join("rsync_upload.sh");

    let cluster_username = &args.cluster_username;
    let cluster_server_address = &args.cluster_address;
    let upload_destination_directory = &args.cluster_upload_destination.display();

    let mut file = std::fs::File::create(&filename)
        .map_err(|e| error::CreateFile::new(e, filename.clone()))?;

    let command = format!("rsync -rlt --safe-links $DIR {cluster_username}@{cluster_server_address}:{upload_destination_directory}");

    file.write_all(b"# run this script to upload the files to the cluster for execution\n")
        .map_err(|e| error::WriteFile::new(e, filename.clone()))?;
    // ensure the current working directory is handled properly
    file.write_all(CURRENT_DUR_BASH)
        .map_err(|e| error::WriteFile::new(e, filename.clone()))?;
    file.write_all(b"\n")
        .map_err(|e| error::WriteFile::new(e, filename.clone()))?;
    file.write_all(command.as_bytes())
        .map_err(|e| error::WriteFile::new(e, filename.clone()))?;

    Ok(())
}

/// write a script to schedule all the jobs sequentially
fn schedule_jobs_sciprt(args: &cli::Slurm, jobs: &[String]) -> Result<(), error::Slurm> {
    let filename = args.output_folder.join("schedule_jobs.sh");

    let mut file = std::fs::File::create(&filename)
        .map_err(|e| error::CreateFile::new(e, filename.clone()))?;

    let srun_command = |job_name: &str| format!("cd $DIR/{job_name}\nsbatch slurm_input.sl");

    writeln!(
        file,
        "# run this script on the SLURM cluster to schedule all the jobs"
    )
    .map_err(|e| error::WriteFile::new(e, filename.clone()))?;

    // ensure the current working directory is handled properly
    file.write_all(CURRENT_DUR_BASH)
        .map_err(|e| error::WriteFile::new(e, filename.clone()))?;

    writeln!(file, "").map_err(|e| error::WriteFile::new(e, filename.clone()))?;

    for job_name in jobs {
        let command = srun_command(&job_name);

        writeln!(file, "{command}\n").map_err(|e| error::WriteFile::new(e, filename.clone()))?;
    }

    Ok(())
}
