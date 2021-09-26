use std::path::{PathBuf, Path};
use crate::transport;
use std::io;
use std::convert::TryFrom;

/// stores job data on disk
pub(crate) enum StoredJob {
    Python(LazyPythonJob),
    Singularity(LazySingularityJob)
}

impl StoredJob {
    async fn from_python(x: transport::PythonJob, output_dir: &Path) -> Result<Self, io::Error> {
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
        tokio::fs::write(&python_setup_file_path, x.python_file).await?;

        // write the required files
        let mut required_files = vec![];

        let mut i = 0;
        for file in x.job_files {
            let path = output_dir.join(&format!("{}_{}_job.dist", hash, i));

            tokio::fs::write(&path, file.file_bytes).await?;

            required_files.push(
                LazyFile {
                    file_name: file.file_name,
                    path
                }
            );

            i +=1
        }

        let job = LazyPythonJob {
            job_name: x.job_name,
            python_setup_file_path,
            required_files,
        };

        Ok(Self::Python(job))
    }

    async fn from_singularity(x: transport::SingularityJob, output_dir: &Path) -> Result<Self, io::Error> {
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

            tokio::fs::write(&path, file.file_bytes).await?;

            required_files.push(
                LazyFile {
                    file_name: file.file_name,
                    path
                }
            );

            i +=1
        }

        let job = LazySingularityJob {
            job_name: x.job_name,
            required_files,
        };

        Ok(Self::Singularity(job))
    }

    pub(crate) async fn load_job(self) -> Result<JobOpt, io::Error> {
        match self {
            Self::Python(x) => Ok(JobOpt::Python(x.load_job().await?)),
            Self::Singularity(x) => Ok(JobOpt::Singularity(x.load_job().await?)),
        }
    }
}

pub(crate) enum JobOpt {
    Singularity(transport::SingularityJob),
    Python(transport::PythonJob),
}

pub(crate) struct LazyPythonJob {
    job_name: String,
    python_setup_file_path: PathBuf,
    required_files: Vec<LazyFile>
}

impl LazyPythonJob {
    pub(crate) async fn load_job(self) -> Result<transport::PythonJob, io::Error> {
        let python_file = tokio::fs::read(&self.python_setup_file_path).await?;
        let mut job_files = vec![];

        for lazy_file in self.required_files {
            let bytes = tokio::fs::read(&lazy_file.path).await?;
            job_files.push(transport::File{ file_name: lazy_file.file_name, file_bytes: bytes });
        }

        Ok(transport::PythonJob {
            job_name: self.job_name,
            python_file,
            job_files
        })
    }
}

pub(crate) struct LazySingularityJob {
    job_name: String,
    required_files: Vec<LazyFile>
}

impl LazySingularityJob {
    pub(crate) async fn load_job(self) -> Result<transport::SingularityJob, io::Error> {
        let job_files = load_files(&self.required_files).await?;

        Ok(transport::SingularityJob{
            job_name: self.job_name,
            job_files
        })
    }
}

pub(crate) struct LazyFile {
    file_name: String,
    path: PathBuf,
}

/// stores initialization data on disk
pub(crate) enum StoredJobInit {
    Python(LazyPythonInit),
    Singularity(LazySingularityInit)
}

impl StoredJobInit {
    async fn from_python(x: transport::PythonJobInit, output_dir: &Path) -> Result<Self, io::Error> {
        // compute the hash
        let mut sha = sha1::Sha1::new();
        sha.update(&x.python_setup_file);
        sha.update(&x.batch_name.as_bytes());
        for file in &x.additional_build_files{
            sha.update(&file.file_name.as_bytes());
            sha.update(&file.file_bytes);
        }

        let hash = sha.digest().to_string();
        
        // write the python setup file

        let python_setup_file_path = output_dir.join(&format!("{}_py_setup.dist", hash));
        tokio::fs::write(&python_setup_file_path, x.python_setup_file).await?;

        // write the required files
        let mut required_files = vec![];

        let mut i = 0;
        for file in x.additional_build_files {
            let path = output_dir.join(&format!("{}_{}_setup.distribute", hash, i));

            tokio::fs::write(&path, file.file_bytes).await?;

            required_files.push(
                LazyFile {
                    file_name: file.file_name,
                    path
                }
            );

            i +=1
        }

        let job = LazyPythonInit {
            batch_name: x.batch_name,
            python_setup_file_path,
            required_files,
        };

        Ok(Self::Python(job))
    }

    async fn from_singularity(x: transport::SingularityJobInit, output_dir: &Path) -> Result<Self, io::Error> {
        // compute the hash 
        let mut sha = sha1::Sha1::new();
        sha.update(&x.batch_name.as_bytes());
        for file in &x.build_files {
            sha.update(&file.file_name.as_bytes());
            sha.update(&file.file_bytes);
        }

        let hash = sha.digest().to_string();

        // write a sif file
        let sif_path= output_dir.join(&format!("{}_setup.distribute", hash));
        tokio::fs::write(&sif_path, &x.sif_bytes).await?;
        
        // write the required files
        let mut required_files = vec![];

        let mut i = 0;
        for file in x.build_files {
            let path = output_dir.join(&format!("{}_{}_setup.distribute", hash, i));

            tokio::fs::write(&path, file.file_bytes).await?;

            required_files.push(
                LazyFile {
                    file_name: file.file_name,
                    path
                }
            );

            i +=1
        }

        let job = LazySingularityInit {
            batch_name: x.batch_name,
            sif_path,
            required_files,
        };

        Ok(Self::Singularity(job))
    }

    pub(crate) async fn load_build(&self) -> Result<JobOptInit, io::Error> {
        match self {
            Self::Python(x) => Ok(JobOptInit::Python(x.load_build().await?)),
            Self::Singularity(x) => Ok(JobOptInit::Singularity(x.load_build().await?)),
        }
    }
}

pub(crate) enum JobOptInit {
    Singularity(transport::SingularityJobInit),
    Python(transport::PythonJobInit),
}

pub(crate) struct LazyPythonInit {
    batch_name: String,
    python_setup_file_path: PathBuf,
    required_files: Vec<LazyFile>
}

impl LazyPythonInit {
    async fn load_build(&self) -> Result< transport::PythonJobInit, io::Error> {

        let python_setup_file = tokio::fs::read(&self.python_setup_file_path).await?;

        let additional_build_files = load_files(&self.required_files).await?;

        let out = transport::PythonJobInit {
            batch_name: self.batch_name.clone(),
            python_setup_file,
            additional_build_files
        };

        Ok(out)
    }
}        

pub(crate) struct LazySingularityInit {
    batch_name: String,
    sif_path: PathBuf,
    required_files: Vec<LazyFile>
}

impl LazySingularityInit {
    async fn load_build(&self) -> Result< transport::SingularityJobInit, io::Error> {

        let sif_bytes = tokio::fs::read(&self.sif_path).await?;

        let build_files = load_files(&self.required_files).await?;

        let out = transport::SingularityJobInit {
            batch_name: self.batch_name.clone(),
            sif_bytes,
            build_files
        };
        Ok(out)
    }
}        

async fn load_files(files: &[LazyFile]) -> Result<Vec<transport::File>, io::Error> {
    let mut job_files = vec![];

    for lazy_file in files {
        let bytes = tokio::fs::read(&lazy_file.path).await?;
        job_files.push(transport::File{ file_name: lazy_file.file_name.clone(), file_bytes: bytes });
    }

    Ok(job_files)
}
