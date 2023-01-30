use crate::prelude::*;

pub fn slurm(args: cli::Slurm) -> Result<(), error::Slurm> {
    //
    // load the config files
    //
    let jobs = config::load_config::<config::Jobs<config::common::File>>(&args.jobs)?;

    if jobs.len_jobs() == 0 {
        return Err(error::Slurm::NoJobs);
    }

    // ensure that there are no duplicate job names
    crate::add::check_has_duplicates(&jobs.job_names())?;


    Ok(())
}
