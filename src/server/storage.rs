use crate::transport;
use std::convert::TryFrom;
use std::io;
use std::path::{Path, PathBuf};

use super::{JobRequiredCaps, Requirements};

use crate::config;
use derive_more::{Constructor, Display, From};
use serde::{Deserialize, Serialize};

/// stores job data on disk
#[derive(Debug)]
pub(crate) enum StoredJob {
    Python(LazyPythonJob),
    Singularity(LazySingularityJob),
}

impl StoredJob {
    pub(crate) fn from_python(
        x: transport::PythonJob,
        output_dir: &Path,
    ) -> Result<Self, io::Error> {
        let mut sha = sha1::Sha1::new();
        sha.update(&x.python_file);
        sha.update(&x.job_name.as_bytes());
        for file in &x.job_files {
            sha.update(&file.file_name.as_bytes());
            sha.update(&file.file_bytes);
        }

        let hash = sha.digest().to_string();

        // write the python setup file

        let python_setup_file_path = output_dir.join(&format!("{}_py_job.dist", hash));
        std::fs::write(&python_setup_file_path, x.python_file)?;

        // write the required files
        let mut required_files = vec![];

        let mut i = 0;
        for file in x.job_files {
            let path = output_dir.join(&format!("{}_{}_job.dist", hash, i));

            std::fs::write(&path, file.file_bytes)?;

            required_files.push(LazyFile {
                file_name: file.file_name,
                path,
            });

            i += 1
        }

        let job = LazyPythonJob {
            job_name: x.job_name,
            python_setup_file_path,
            required_files,
        };

        Ok(Self::Python(job))
    }

    pub(crate) fn from_singularity(
        x: transport::SingularityJob,
        output_dir: &Path,
    ) -> Result<Self, io::Error> {
        // compute the hash
        let mut sha = sha1::Sha1::new();
        sha.update(&x.job_name.as_bytes());
        for file in &x.job_files {
            sha.update(&file.file_name.as_bytes());
            sha.update(&file.file_bytes);
        }

        let hash = sha.digest().to_string();

        // write the required files
        let mut required_files = vec![];

        let mut i = 0;
        for file in x.job_files {
            let path = output_dir.join(&format!("{}_{}_job.distribute", hash, i));

            std::fs::write(&path, file.file_bytes)?;

            required_files.push(LazyFile {
                file_name: file.file_name,
                path,
            });

            i += 1
        }

        let job = LazySingularityJob {
            job_name: x.job_name,
            required_files,
        };

        Ok(Self::Singularity(job))
    }

    pub(crate) fn from_opt(x: JobOpt, path: &Path) -> Result<Self, io::Error> {
        match x {
            JobOpt::Python(python) => Self::from_python(python, path),
            JobOpt::Singularity(sing) => Self::from_singularity(sing, path),
        }
    }

    pub(crate) fn load_job(self) -> Result<JobOpt, io::Error> {
        match self {
            Self::Python(x) => Ok(JobOpt::Python(x.load_job()?)),
            Self::Singularity(x) => Ok(JobOpt::Singularity(x.load_job()?)),
        }
    }

    pub(crate) fn job_name(&self) -> &str {
        match &self {
            Self::Python(python) => &python.job_name,
            Self::Singularity(sing) => &sing.job_name,
        }
    }
}

#[derive(Clone, PartialEq, From, Debug)]
pub(crate) enum JobOpt {
    Singularity(transport::SingularityJob),
    Python(transport::PythonJob),
}

impl JobOpt {
    pub(crate) fn name(&self) -> &str {
        match &self {
            Self::Singularity(x) => &x.job_name,
            Self::Python(x) => &x.job_name,
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
pub(crate) struct LazySingularityJob {
    job_name: String,
    required_files: Vec<LazyFile>,
}

impl LazySingularityJob {
    pub(crate) fn load_job(self) -> Result<transport::SingularityJob, io::Error> {
        let job_files = load_files(&self.required_files, true)?;

        Ok(transport::SingularityJob {
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
    Singularity(LazySingularityInit),
}

impl StoredJobInit {
    pub(crate) fn from_python(
        x: transport::PythonJobInit,
        output_dir: &Path,
    ) -> Result<Self, io::Error> {
        // compute the hash
        let mut sha = sha1::Sha1::new();
        sha.update(&x.python_setup_file);
        sha.update(&x.batch_name.as_bytes());
        for file in &x.additional_build_files {
            sha.update(&file.file_name.as_bytes());
            sha.update(&file.file_bytes);
        }

        let hash = sha.digest().to_string();

        // write the python setup file

        let python_setup_file_path = output_dir.join(&format!("{}_py_setup.dist", hash));
        std::fs::write(&python_setup_file_path, x.python_setup_file)?;

        // write the required files
        let mut required_files = vec![];

        let mut i = 0;
        for file in x.additional_build_files {
            let path = output_dir.join(&format!("{}_{}_setup.distribute", hash, i));

            std::fs::write(&path, file.file_bytes)?;

            required_files.push(LazyFile {
                file_name: file.file_name,
                path,
            });

            i += 1
        }

        let job = LazyPythonInit {
            batch_name: x.batch_name,
            python_setup_file_path,
            required_files,
        };

        Ok(Self::Python(job))
    }

    pub(crate) fn from_singularity(
        x: transport::SingularityJobInit,
        output_dir: &Path,
    ) -> Result<Self, io::Error> {
        // compute the hash
        let mut sha = sha1::Sha1::new();
        sha.update(&x.batch_name.as_bytes());
        for file in &x.build_files {
            sha.update(&file.file_name.as_bytes());
            sha.update(&file.file_bytes);
        }

        let hash = sha.digest().to_string();

        // write a sif file
        let sif_path = output_dir.join(&format!("{}_setup.distribute", hash));
        std::fs::write(&sif_path, &x.sif_bytes)?;

        // write the required files
        let mut required_files = vec![];

        let mut i = 0;
        for file in x.build_files {
            let path = output_dir.join(&format!("{}_{}_setup.distribute", hash, i));

            std::fs::write(&path, file.file_bytes)?;

            required_files.push(LazyFile {
                file_name: file.file_name,
                path,
            });

            i += 1
        }

        let job = LazySingularityInit {
            batch_name: x.batch_name,
            sif_path,
            required_files,
        };

        Ok(Self::Singularity(job))
    }

    pub(crate) fn load_build(&self) -> Result<config::BuildOpts, io::Error> {
        match self {
            Self::Python(x) => Ok(config::BuildOpts::Python(x.load_build()?)),
            Self::Singularity(x) => Ok(config::BuildOpts::Singularity(x.load_build()?)),
        }
    }

    pub(crate) fn delete(&self) -> Result<(), io::Error> {
        match self {
            Self::Python(x) => x.delete()?,
            Self::Singularity(x) => x.delete()?,
        };

        Ok(())
    }

    pub(crate) fn from_opt(opt: config::BuildOpts, output_dir: &Path) -> Result<Self, io::Error> {
        match opt {
            config::BuildOpts::Singularity(s) => Self::from_singularity(s, output_dir),
            config::BuildOpts::Python(s) => Self::from_python(s, output_dir),
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
pub(crate) struct LazySingularityInit {
    batch_name: String,
    sif_path: PathBuf,
    required_files: Vec<LazyFile>,
}

impl LazySingularityInit {
    fn load_build(&self) -> Result<transport::SingularityJobInit, io::Error> {
        let sif_bytes = std::fs::read(&self.sif_path)?;

        let build_files = load_files(&self.required_files, false)?;

        let out = transport::SingularityJobInit {
            batch_name: self.batch_name.clone(),
            sif_bytes,
            build_files,
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
pub(crate) struct OwnedJobSet {
    pub(crate) build: config::BuildOpts,
    pub(crate) requirements: Requirements<JobRequiredCaps>,
    pub(crate) remaining_jobs: config::JobOpts,
    pub(crate) currently_running_jobs: usize,
    pub(crate) batch_name: String,
    pub(crate) matrix_user: Option<matrix_notify::UserId>,
    pub(crate) namespace: String,
}
