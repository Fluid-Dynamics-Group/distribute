use crate::transport;

use crate::prelude::*;
use std::io;

use crate::config;
use transport::JobOpt;

use sha1::Digest;

/// stores job data on disk
#[derive(Debug)]
pub(crate) enum StoredJob {
    Python(LazyPythonJob),
    Apptainer(LazyApptainerJob),
}

impl StoredJob {
    pub(crate) fn from_python(
        job: &config::python::Job,
        distribute_input_files_base_path: &Path,
    ) -> Result<Self, io::Error> {
        let job_name = job.name().to_string();
        let python_setup_file_path = job.python_job_file().to_owned();
        let required_files = job.required_files().into_iter()
            .map(|file| {
                LazyFile {
                    // alias will have been properly set on the `add`
                    file_name: file.filename().unwrap(),
                    path: distribute_input_files_base_path.join(file.path()),
                }
            }).collect();

        let job = LazyPythonJob {
            job_name,
            python_setup_file_path,
            required_files,
        };

        Ok(Self::Python(job))
    }

    pub(crate) fn from_apptainer(
        job: &config::apptainer::Job,
        distribute_input_files_base_path: &Path,
    ) -> Result<Self, io::Error> {
        let job_name = job.name().to_string();

        let required_files = job.required_files().into_iter()
            .map(|file| {
                LazyFile {
                    // alias will have been properly set on the `add`
                    file_name: file.filename().unwrap(),
                    path: distribute_input_files_base_path.join(file.path()),
                }
            }).collect();

        let job = LazyApptainerJob {
            job_name,
            required_files,
        };

        Ok(Self::Apptainer(job))
    }

    //pub(crate) fn from_opt(config: &config::Jobs, path: &Path) -> Result<Self, io::Error> {
    //    match config {
    //        config::Jobs::Python(python) => Self::from_python(python, path),
    //        config::Jobs::Apptainer(sing) => Self::from_apptainer(sing, path),
    //    }
    //}

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

#[derive(Debug, PartialEq, Eq)]
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

#[derive(Debug, PartialEq, Eq)]
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

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct LazyFile {
    file_name: String,
    path: PathBuf,
}

/// stores initialization data on disk
#[derive(Debug)]
pub(crate) enum StoredJobInit {
    Python(LazyPythonInit),
    Apptainer(LazyApptainerInit),
}

impl StoredJobInit {
    pub(crate) fn from_python(
        config: &config::PythonConfig,
        distribute_input_files_base_path: &Path
    ) -> Self {
        let batch_name = config.meta().batch_name().to_string();
        let initialize = config.description().initialize();
        let python_setup_file_path = initialize.python_build_file_path().to_owned();
        let required_files = initialize
            .required_files()
            .into_iter()
            .map(|file| {
                LazyFile {
                    // alias will have been properly set on the `add`
                    file_name: file.filename().unwrap(),
                    path: distribute_input_files_base_path.join(file.path()),
                }
            })
            .collect();

        let lazy = LazyPythonInit {
            batch_name,
            required_files,
            python_setup_file_path,
        };

        StoredJobInit::Python(lazy)
    }

    pub(crate) fn from_apptainer(
        config: &config::ApptainerConfig,
        distribute_input_files_base_path: &Path
    ) -> Self {
        let batch_name = config.meta().batch_name().to_string();
        let initialize = config.description().initialize();

        let sif_path = initialize.sif.clone();
        let required_files = initialize.required_files.iter()
            .map(|file| {
                LazyFile {
                    // alias will have been properly set on the `add`
                    file_name: file.filename().unwrap(),
                    path: distribute_input_files_base_path.join(file.path()),
                }
            }).collect();

        let container_bind_paths = initialize.required_mounts.clone();

        let lazy = LazyApptainerInit {
            batch_name,
            sif_path,
            required_files,
            container_bind_paths
        };

        StoredJobInit::Apptainer(lazy)
    }

    pub(crate) fn load_build(&self) -> Result<transport::BuildOpts, io::Error> {
        match self {
            Self::Python(x) => Ok(transport::BuildOpts::Python(x.load_build()?)),
            Self::Apptainer(x) => Ok(transport::BuildOpts::Apptainer(x.load_build()?)),
        }
    }

    pub(crate) fn delete(&self) -> Result<(), io::Error> {
        match self {
            Self::Python(x) => x.delete()?,
            Self::Apptainer(x) => x.delete()?,
        };

        Ok(())
    }

    pub(crate) fn from_opt(
        opt: &config::Jobs, 
        distribute_input_files_base_path: &Path
    ) -> Self {
        match opt {
            config::Jobs::Apptainer(s) => Self::from_apptainer(s, distribute_input_files_base_path),
            config::Jobs::Python(s) => Self::from_python(&s, distribute_input_files_base_path),
        }
    }
}

#[derive(Debug)]
pub(crate) struct LazyPythonInit {
    batch_name: String,
    python_setup_file_path: PathBuf,
    required_files: Vec<LazyFile>,
}

impl LazyPythonInit {
    fn load_build(&self) -> Result<transport::PythonJobInit, io::Error> {
        let python_setup_file = std::fs::read(&self.python_setup_file_path)?;

        let additional_build_files = load_files(&self.required_files, false)?;

        let out = transport::PythonJobInit {
            batch_name: self.batch_name.clone(),
            python_setup_file,
            additional_build_files,
        };

        Ok(out)
    }

    fn delete(&self) -> Result<(), io::Error> {
        std::fs::remove_file(&self.python_setup_file_path)?;
        for file in &self.required_files {
            std::fs::remove_file(&file.path)?;
        }
        Ok(())
    }
}

#[derive(Debug)]
pub(crate) struct LazyApptainerInit {
    batch_name: String,
    sif_path: PathBuf,
    required_files: Vec<LazyFile>,
    container_bind_paths: Vec<PathBuf>,
}

impl LazyApptainerInit {
    fn load_build(&self) -> Result<transport::ApptainerJobInit, io::Error> {
        let sif_bytes = std::fs::read(&self.sif_path)?;

        let build_files = load_files(&self.required_files, false)?;

        let out = transport::ApptainerJobInit {
            batch_name: self.batch_name.clone(),
            sif_bytes,
            build_files,
            container_bind_paths: self.container_bind_paths.clone(),
        };
        Ok(out)
    }

    fn delete(&self) -> Result<(), io::Error> {
        std::fs::remove_file(&self.sif_path)?;

        for file in &self.required_files {
            std::fs::remove_file(&file.path)?;
        }

        Ok(())
    }
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

#[derive(Constructor, Debug, Clone, Deserialize, Serialize)]
pub struct OwnedJobSet {
    pub(crate) build: transport::BuildOpts,
    pub(crate) requirements:
        config::requirements::Requirements<config::requirements::JobRequiredCaps>,
    pub(crate) remaining_jobs: config::JobOpts,
    pub(crate) batch_name: String,
    pub(crate) matrix_user: Option<matrix_notify::OwnedUserId>,
    pub(crate) namespace: String,
}
