use super::cli;
use super::cli::Template;
use crate::config::{self, common, python, apptainer};
use crate::error::{Error, TemplateError};

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
        "/path/to/build.py".into(),
        vec![
            common::File {
                path: "/file/always/present/1.txt".into(),
                alias: Some("optional_alias.txt".to_string()),
            },
            common::File {
                path: "/another/file/2.json".into(),
                alias: None,
            },
            common::File {
                path: "/maybe/python/utils_file.py".into(),
                alias: None,
            },
        ],
    );

    let job_1 = python::Job::new(
        "job_1".into(),
        "execute_job.py".into(),
        vec![
            common::File {
                path: "job_configuration_file.json".into(),
                alias: None,
            },
            common::File {
                path: "job_configuration_file_with_alias.json".into(),
                alias: Some("input.json".to_string()),
            },
        ],
    );

    let python = python::Description::new(initialize, vec![job_1]);
    let meta = meta();
    let desc = config::Jobs::Python { meta, python };
    let serialized = serde_yaml::to_string(&desc)?;

    Ok(serialized)
}

fn apptainer_template() -> Result<String, TemplateError> {
    let initialize = apptainer::Initialize::new(
        "execute_container.sif".into(),
        vec![
            common::File {
                path: "/file/always/present/1.txt".into(),
                alias: Some("optional_alias.txt".to_string()),
            },
            common::File {
                path: "/another/file/2.json".into(),
                alias: None,
            },
            common::File {
                path: "/maybe/python/utils_file.py".into(),
                alias: None,
            },
        ],
        vec!["/path/inside/container/to/mount".into()],
    );

    let job_1 = apptainer::Job::new(
        "job_1".into(),
        vec![
            common::File {
                path: "job_configuration_file.json".into(),
                alias: None,
            },
            common::File {
                path: "job_configuration_file_with_alias.json".into(),
                alias: Some("input.json".to_string()),
            },
        ],
    );

    let apptainer = apptainer::Description::new(initialize, vec![job_1]);
    let meta = meta();
    let desc = config::Jobs::Apptainer { meta, apptainer };
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
