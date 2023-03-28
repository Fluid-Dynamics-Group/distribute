use super::cli;
use super::cli::Template;
use crate::config::{self, apptainer, common, python};
use crate::error::{Error, TemplateError};

/// create a `distribute-jobs.yaml` with some default fields
pub fn template(args: Template) -> Result<(), Error> {
    let out = to_template(args.mode)?;

    std::fs::write(&args.output, out.as_bytes()).map_err(TemplateError::from)?;

    Ok(())
}

fn to_template(template: cli::TemplateType) -> Result<String, TemplateError> {
    match template {
        cli::TemplateType::Python => python_template(),
        cli::TemplateType::Apptainer => apptainer_template(),
    }
}

fn python_template() -> Result<String, TemplateError> {
    let initialize = python::Initialize::new(
        common::File::new_relative("/path/to/build.py"),
        vec![
            common::File::with_alias_relative("/file/always/present/1.txt", "optional_alias.txt"),
            common::File::new_relative("/another/file/2.json"),
            common::File::new_relative("/maybe/python/utils_file.py"),
        ],
    );

    let job_1 = python::Job::new(
        "job_1".into(),
        common::File::new_relative("execute_job.py"),
        vec![
            common::File::new_relative("job_configuration_file.json"),
            common::File::with_alias_relative(
                "job_configuration_file_with_alias.json",
                "input.json",
            ),
        ],
        None,
    );

    let python = python::Description::new(initialize, vec![job_1]);
    let meta = meta();

    // TODO: better slurm documentation
    let slurm = None;
    let desc: config::Jobs<common::File> = config::PythonConfig::new(meta, python, slurm).into();

    let serialized = serde_yaml::to_string(&desc)?;

    Ok(serialized)
}

fn apptainer_template() -> Result<String, TemplateError> {
    let initialize = apptainer::Initialize::new(
        common::File::new_relative("execute_container.sif"),
        vec![
            common::File::with_alias_relative("/file/always/present/1.txt", "optional_alias.txt"),
            common::File::new_relative("/another/file/2.json"),
            common::File::new_relative("/maybe/python/utils_file.py"),
        ],
        vec!["/path/inside/container/to/mount".into()],
    );

    let job_1 = apptainer::Job::new(
        "job_1".into(),
        vec![
            common::File::new_relative("job_configuration_file.json"),
            common::File::with_alias_relative(
                "job_configuration_file_with_alias.json",
                "input.json",
            ),
        ],
        // No slurm information
        None,
    );

    let apptainer = apptainer::Description::new(initialize, vec![job_1]);
    let meta = meta();

    // TODO: better slurm defaults
    let slurm = None;
    let desc: config::Jobs<common::File> =
        config::ApptainerConfig::new(meta, apptainer, slurm).into();
    let serialized = serde_yaml::to_string(&desc)?;

    Ok(serialized)
}

fn meta() -> config::Meta {
    config::Meta {
        batch_name: "your_jobset_name".into(),
        namespace: "example_namespace".into(),
        matrix: None,
        capabilities: vec!["python3", "apptainer", "gfortran"]
            .into_iter()
            .map(Into::into)
            .collect(),
    }
}

#[test]
fn create_apptainer_template() {
    apptainer_template().unwrap();
}

#[test]
fn create_python_template() {
    apptainer_template().unwrap();
}
