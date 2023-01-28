use crate::config;
use config::common::File;
use std::path::PathBuf;

use sha1::Digest;

pub(super) trait HashableComponent {
    /// either python compilation file, or the singularity .sif file
    fn job_file(&self) -> PathBuf;
    /// name of the job
    fn job_name(&self) -> &str;
    /// all the files associated with this component
    fn files(&self) -> Box<dyn Iterator<Item = PathBuf> + '_>;
}

impl HashableComponent for config::apptainer::Initialize<File> {
    fn job_file(&self) -> PathBuf {
        self.sif.path().to_owned()
    }
    fn job_name(&self) -> &str {
        "initialize"
    }
    fn files(&self) -> Box<dyn Iterator<Item = PathBuf> + '_> {
        let iter = (&self.required_files)
            .into_iter()
            .map(|f| f.path().to_owned());

        Box::new(iter)
    }
}

impl HashableComponent for config::python::Initialize<File> {
    fn job_file(&self) -> PathBuf {
        self.python_build_file_path().path().to_owned()
    }
    fn job_name(&self) -> &str {
        "initialize"
    }
    fn files(&self) -> Box<dyn Iterator<Item = PathBuf> + '_> {
        let iter = self
            .required_files()
            .into_iter()
            .map(|f| f.path().to_owned());

        Box::new(iter)
    }
}

impl HashableComponent for config::apptainer::Job<File> {
    fn job_file(&self) -> PathBuf {
        PathBuf::from("job")
    }
    fn job_name(&self) -> &str {
        self.name()
    }
    fn files(&self) -> Box<dyn Iterator<Item = PathBuf> + '_> {
        let iter = self
            .required_files()
            .into_iter()
            .map(|f| f.path().to_owned());

        Box::new(iter)
    }
}

impl HashableComponent for config::python::Job<File> {
    fn job_file(&self) -> PathBuf {
        self.python_job_file().path().to_owned()
    }
    fn job_name(&self) -> &str {
        self.name()
    }
    fn files(&self) -> Box<dyn Iterator<Item = PathBuf> + '_> {
        let iter = self
            .required_files()
            .into_iter()
            .map(|f| f.path().to_owned());

        Box::new(iter)
    }
}

pub(super) fn filename_hash<T: HashableComponent>(data: &T) -> String {
    let mut sha = sha1::Sha1::new();

    sha.update(data.job_name().as_bytes());

    for file in data.files() {
        let filename = file.file_name().unwrap().to_string_lossy();
        sha.update(filename.as_bytes());
        let bytes = std::fs::read(file).unwrap();
        sha.update(&bytes);
    }

    let hash = base16::encode_lower(&sha.finalize());

    hash
}
