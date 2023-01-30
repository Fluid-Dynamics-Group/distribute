pub use distribute::common::File;

use std::path::PathBuf;

#[derive(Debug, Clone)]
#[pyo3::pyclass]
pub struct ApptainerConfig {
    pub meta: Meta,
    pub description: Description,
}

impl ApptainerConfig {
    pub fn into_distribute_version(self) -> distribute::ApptainerConfig<File> {
        let Meta {
            batch_name,
            namespace,
            matrix,
            capabilities,
        } = self.meta;
        let Description { initialize, jobs } = self.description;
        let Initialize {
            sif,
            required_files,
            required_mounts,
        } = initialize;

        let capabilities = capabilities.into_iter().map(Into::into).collect();
        let initialize =
            distribute::apptainer::Initialize::new(sif, required_files, required_mounts);

        let jobs = jobs
            .into_iter()
            .map(|job| {
                let Job {
                    name,
                    required_files,
                } = job;
                distribute::apptainer::Job::new(name, required_files)
            })
            .collect();

        let meta = distribute::Meta::new(batch_name, namespace, matrix, capabilities);
        let description = distribute::apptainer::Description::new(initialize, jobs);

        distribute::ApptainerConfig::new(meta, description)
    }
}

#[derive(Debug, Clone)]
#[pyo3::pyclass]
pub struct Description {
    pub initialize: Initialize,
    pub jobs: Vec<Job>,
}

#[derive(Debug, Clone)]
#[pyo3::pyclass]
pub struct Initialize {
    pub sif: File,
    pub required_files: Vec<File>,
    pub required_mounts: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
#[pyo3::pyclass]
pub struct Job {
    pub name: String,
    pub required_files: Vec<File>,
}

#[derive(Debug, Clone)]
#[pyo3::pyclass]
pub struct Meta {
    pub batch_name: String,
    pub namespace: String,
    pub matrix: Option<distribute::OwnedUserId>,
    pub capabilities: Vec<String>,
}
