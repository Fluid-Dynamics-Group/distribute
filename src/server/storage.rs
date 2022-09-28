use crate::transport;

use crate::prelude::*;
use std::io;

use crate::config;
use transport::JobOpt;

/// stores job data on disk
#[derive(Debug)]
pub(crate) enum StoredJob {
    Python(LazyPythonJob),
    Apptainer(LazyApptainerJob),
}

impl StoredJob {
    pub(crate) fn from_python(
        x: transport::PythonJob,
        output_dir: &Path,
    ) -> Result<Self, io::Error> {
        let mut sha = sha1::Sha1::new();
        sha.update(&x.python_file);
        sha.update(x.job_name.as_bytes());
        for file in &x.job_files {
            sha.update(file.file_name.as_bytes());
            sha.update(&file.file_bytes);
        }

        let hash = sha.digest().to_string();

        // write the python setup file

        let python_setup_file_path = output_dir.join(&format!("{}_py_job.dist", hash));

        trace!(
            "saving python lazy file to {}, current working directory is {}",
            python_setup_file_path.display(),
            std::env::current_dir().unwrap().display()
        );

        std::fs::write(&python_setup_file_path, x.python_file)?;

        // write the required files
        let mut required_files = vec![];

        for (i, file) in x.job_files.into_iter().enumerate() {
            let path = output_dir.join(&format!("{}_{}_job.dist", hash, i));

            trace!("saving python dependent file to {}", path.display());

            std::fs::write(&path, file.file_bytes)?;

            required_files.push(LazyFile {
                file_name: file.file_name,
                path,
            });
        }

        let job = LazyPythonJob {
            job_name: x.job_name,
            python_setup_file_path,
            required_files,
        };

        Ok(Self::Python(job))
    }

    pub(crate) fn from_apptainer(
        x: transport::ApptainerJob,
        output_dir: &Path,
    ) -> Result<Self, io::Error> {
        // compute the hash
        let mut sha = sha1::Sha1::new();
        sha.update(x.job_name.as_bytes());
        for file in &x.job_files {
            sha.update(file.file_name.as_bytes());
            sha.update(&file.file_bytes);
        }

        let hash = sha.digest().to_string();

        // write the required files
        let mut required_files = vec![];

        for (i, file) in x.job_files.into_iter().enumerate() {
            let path = output_dir.join(&format!("{}_{}_job.distribute", hash, i));

            std::fs::write(&path, file.file_bytes)?;

            required_files.push(LazyFile {
                file_name: file.file_name,
                path,
            });
        }

        let job = LazyApptainerJob {
            job_name: x.job_name,
            required_files,
        };

        Ok(Self::Apptainer(job))
    }

    pub(crate) fn from_opt(x: JobOpt, path: &Path) -> Result<Self, io::Error> {
        match x {
            JobOpt::Python(python) => Self::from_python(python, path),
            JobOpt::Apptainer(sing) => Self::from_apptainer(sing, path),
        }
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
        x: transport::PythonJobInit,
        output_dir: &Path,
    ) -> Result<Self, io::Error> {
        // compute the hash
        let mut sha = sha1::Sha1::new();
        sha.update(&x.python_setup_file);
        sha.update(x.batch_name.as_bytes());
        for file in &x.additional_build_files {
            sha.update(file.file_name.as_bytes());
            sha.update(&file.file_bytes);
        }

        let hash = sha.digest().to_string();

        // write the python setup file

        let python_setup_file_path = output_dir.join(&format!("{}_py_setup.dist", hash));
        std::fs::write(&python_setup_file_path, x.python_setup_file)?;

        // write the required files
        let mut required_files = vec![];

        for (i, file) in x.additional_build_files.into_iter().enumerate() {
            let path = output_dir.join(&format!("{}_{}_setup.distribute", hash, i));

            std::fs::write(&path, file.file_bytes)?;

            required_files.push(LazyFile {
                file_name: file.file_name,
                path,
            });
        }

        let job = LazyPythonInit {
            batch_name: x.batch_name,
            python_setup_file_path,
            required_files,
        };

        Ok(Self::Python(job))
    }

    pub(crate) fn from_apptainer(
        x: transport::ApptainerJobInit,
        output_dir: &Path,
    ) -> Result<Self, io::Error> {
        // compute the hash
        let mut sha = sha1::Sha1::new();
        sha.update(x.batch_name.as_bytes());
        for file in &x.build_files {
            sha.update(file.file_name.as_bytes());
            sha.update(&file.file_bytes);
        }

        let hash = sha.digest().to_string();

        // write a sif file
        let sif_path = output_dir.join(&format!("{}_setup.distribute", hash));
        std::fs::write(&sif_path, &x.sif_bytes)?;

        // write the required files
        let mut required_files = vec![];

        for (i, file) in x.build_files.into_iter().enumerate() {
            let path = output_dir.join(&format!("{}_{}_setup.distribute", hash, i));

            std::fs::write(&path, file.file_bytes)?;

            required_files.push(LazyFile {
                file_name: file.file_name,
                path,
            });
        }

        let job = LazyApptainerInit {
            batch_name: x.batch_name,
            sif_path,
            required_files,
            container_bind_paths: x.container_bind_paths,
        };

        Ok(Self::Apptainer(job))
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
        opt: transport::BuildOpts,
        output_dir: &Path,
    ) -> Result<Self, io::Error> {
        match opt {
            transport::BuildOpts::Apptainer(s) => Self::from_apptainer(s, output_dir),
            transport::BuildOpts::Python(s) => Self::from_python(s, output_dir),
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
    pub(crate) currently_running_jobs: usize,
    pub(crate) batch_name: String,
    pub(crate) matrix_user: Option<matrix_notify::OwnedUserId>,
    pub(crate) namespace: String,
}
