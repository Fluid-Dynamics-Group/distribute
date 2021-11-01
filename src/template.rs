use super::cli::Template;
use crate::error::{Error, TemplateError};
use crate::config::{python, singularity, common};
use std::path::PathBuf;

pub(crate) fn template(args: Template) -> Result<(), Error> {
    let mode = 
        if !args.python && !args.singularity {
            Mode::Singularity
        } else if args.python {
            Mode::Python
        } else if args.singularity {
            Mode::Singularity
        } else {
            unreachable!()
        };
    
    let out = mode.to_template()?;

    std::fs::write(&args.output, out.as_bytes()).map_err(TemplateError::from)?;

    Ok(())
}

enum Mode {
    Singularity,
    Python
}
impl Mode {
    fn to_template(self) -> Result<String, TemplateError> {
        match self {
            Self::Python => python_template(),
            Self::Singularity => singularity_template()
        }
    }
}


fn python_template() -> Result<String, TemplateError> {
    let initialize = python::Initialize::new(
        "/path/to/build.py".into(),
        vec![
            common::File {
                path : "/file/always/present/1.txt".into(),
                alias: Some("optional_alias.txt".to_string())
            },
            common::File {
                path : "/another/file/2.json".into(),
                alias: None
            },
            common::File {
                path : "/maybe/python/utils_file.py".into(),
                alias: None
            },
        ]
    );

    let job_1 = python::Job::new(
        "job_1".into(),
        "execute_job.py".into(),
        vec![
            common::File {
                path : "job_configuration_file.json".into(),
                alias: None
            },
            common::File {
                path : "job_configuration_file_with_alias.json".into(),
                alias: Some("input.json".to_string())
            },
        ]
    );

    let desc = python::Description::new(initialize, vec![job_1] );
    let serialized = serde_yaml::to_string(&desc)?;

    Ok(serialized)
}

fn singularity_template() -> Result<String, TemplateError> {
    let initialize = singularity::Initialize::new(
        "execute_container.sif".into(),
        vec![
            common::File {
                path : "/file/always/present/1.txt".into(),
                alias: Some("optional_alias.txt".to_string())
            },
            common::File {
                path : "/another/file/2.json".into(),
                alias: None
            },
            common::File {
                path : "/maybe/python/utils_file.py".into(),
                alias: None
            },
        ],
        vec!["/path/inside/container/to/mount".into()]
    );

    let job_1 = singularity::Job::new(
        "job_1".into(),
        vec![
            common::File {
                path : "job_configuration_file.json".into(),
                alias: None
            },
            common::File {
                path : "job_configuration_file_with_alias.json".into(),
                alias: Some("input.json".to_string())
            },
        ]
    );

    let desc = singularity::Description::new(initialize, vec![job_1] );
    let serialized = serde_yaml::to_string(&desc)?;

    Ok(serialized)
}
