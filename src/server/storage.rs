use crate::transport;

use crate::prelude::*;
use std::io;

use crate::config;
use transport::JobOpt;

use config::common::HashedFile;

/// stores job data on disk
#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) enum StoredJob {
    Python(LazyPythonJob),
    Apptainer(LazyApptainerJob),
}

impl StoredJob {
    pub(crate) fn from_python(job: &config::python::Job<HashedFile>) -> Result<Self, io::Error> {
        let job_name = job.name().to_string();
        let python_setup_file_path = job.python_job_file().to_owned();
        let required_files = job
            .required_files()
            .into_iter()
            .map(HashedFile::lazy_file_unchecked)
            .collect();

        let job = LazyPythonJob {
            job_name,
            python_setup_file_path,
            required_files,
        };

        Ok(Self::Python(job))
    }

    pub(crate) fn from_apptainer(
        job: &config::apptainer::Job<HashedFile>,
    ) -> Result<Self, io::Error> {
        let job_name = job.name().to_string();

        let required_files = job
            .required_files()
            .into_iter()
            .map(HashedFile::lazy_file_unchecked)
            .collect();

        let job = LazyApptainerJob {
            job_name,
            required_files,
        };

        Ok(Self::Apptainer(job))
    }

    pub(crate) fn load_job(self) -> Result<JobOpt, io::Error> {
        match self {
            Self::Python(x) => Ok(JobOpt::Python(x.load_job()?)),
            Self::Apptainer(x) => Ok(JobOpt::Apptainer(x.load_job()?)),
        }
    }

    pub(crate) fn job_name(&self) -> &str {
        match &self {
            Self::Python(python) => &python.job_name,
            Self::Apptainer(sing) => &sing.job_name,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Clone)]
pub(crate) struct LazyPythonJob {
    job_name: String,
    python_setup_file_path: PathBuf,
    required_files: Vec<LazyFile>,
}

impl LazyPythonJob {
    pub(crate) fn load_job(self) -> Result<transport::PythonJob, io::Error> {
        let python_file = std::fs::read(&self.python_setup_file_path)?;

        let job_files = load_files(&self.required_files, true)?;

        Ok(transport::PythonJob {
            job_name: self.job_name,
            python_file,
            job_files,
        })
    }
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Clone)]
pub(crate) struct LazyApptainerJob {
    job_name: String,
    required_files: Vec<LazyFile>,
}

impl LazyApptainerJob {
    pub(crate) fn load_job(self) -> Result<transport::ApptainerJob, io::Error> {
        let job_files = load_files(&self.required_files, true)?;

        Ok(transport::ApptainerJob {
            job_name: self.job_name,
            job_files,
        })
    }
}

#[derive(Debug, PartialEq, Eq, Constructor, Serialize, Deserialize, Clone)]
pub(crate) struct LazyFile {
    file_name: String,
    path: PathBuf,
}

fn load_files(files: &[LazyFile], delete: bool) -> Result<Vec<transport::File>, io::Error> {
    let mut job_files = vec![];

    for lazy_file in files {
        let bytes = std::fs::read(&lazy_file.path)?;
        job_files.push(transport::File {
            file_name: lazy_file.file_name.clone(),
            file_bytes: bytes,
        });

        if delete {
            std::fs::remove_file(&lazy_file.path)?;
        }
    }

    Ok(job_files)
}
