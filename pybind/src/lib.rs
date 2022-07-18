use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use std::path::PathBuf;

use distribute::common::File;
use distribute::apptainer::Description;
use distribute::apptainer::Initialize;
use distribute::apptainer::Job;
use distribute::ApptainerConfig;
use distribute::Meta;

#[pyfunction]
fn metadata(
    namespace: String,
    batch_name: String,
    capabilities: Vec<String>,
    matrix_username: Option<String>,
) -> PyResult<Meta> {
    let capabilities = capabilities.into_iter().map(|x| x.into()).collect();

    let matrix = if let Some(user) = matrix_username {
        Some(user.parse().map_err(|e| PyValueError::new_err(format!("failed to parse matrix username. Is it in the form of `@youname:matrix_server`? Full Error {e}")))?)
    } else {
        None
    };

    let meta = distribute::Meta {
        batch_name,
        namespace,
        capabilities,
        matrix,
    };
    Ok(meta)
}

#[pyfunction]
fn initialize(
    sif_path: String,
    required_files: Vec<File>,
    required_mounts: Vec<String>,
) -> Initialize {
    distribute::apptainer::Initialize::new(
        PathBuf::from(sif_path),
        required_files,
        required_mounts.into_iter().map(PathBuf::from).collect(),
    )
}

#[pyfunction]
fn job(name: String, required_files: Vec<File>) -> Job {
    distribute::apptainer::Job::new(name, required_files)
}

#[pyfunction]
fn description(initialize: Initialize, jobs: Vec<Job>) -> Description {
    distribute::apptainer::Description::new(initialize, jobs)
}

#[pyfunction(relative="false", alias="None")]
fn file(path: PathBuf, relative: bool, alias: Option<String>) -> PyResult<distribute::common::File> {
    
    let file = match (relative, alias) {
        (true, Some(alias)) => {
            distribute::common::File::with_alias_relative(path, alias)
        }
        (false, Some(alias)) => {
            distribute::common::File::with_alias(path, alias).map_err(|e| PyValueError::new_err(format!("Failed to canonicalize the path provided. If this directory does not exist yet, you should use relative=True. Full error {e}")))?
        }
        (true, None) => {
            distribute::common::File::new_relative(path)
        }
        (false, None) => {
            distribute::common::File::new(path).map_err(|e| PyValueError::new_err(format!("Failed to canonicalize the path provided. If this directory does not exist yet, you should use relative=True. Full error {e}")))?
        }
    };

    Ok(file)
}

#[pyfunction]
fn apptainer_config(meta: Meta, description: Description) -> ApptainerConfig {
    ApptainerConfig::new(meta, description)
}

#[pyfunction]
fn write_config_to_file(config: ApptainerConfig, path: PathBuf) -> PyResult<()> {
    let file = std::fs::File::create(&path)
        .map_err(|e| PyValueError::new_err(format!("failed to create file at {}. Full error: {e}", path.display())))?;

    distribute::Jobs::from(config).to_writer(file)
        .map_err(|e| PyValueError::new_err(format!("failed to serialize contents to file. Full error: {e}")))?;

    Ok(())
}

/// A Python module implemented in Rust. The name of this function must match
/// the `lib.name` setting in the `Cargo.toml`, else Python will not be able to
/// import the module.
#[pymodule]
fn distribute_config(_py: Python<'_>, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(metadata, m)?)?;
    m.add_function(wrap_pyfunction!(initialize, m)?)?;
    m.add_function(wrap_pyfunction!(job, m)?)?;
    m.add_function(wrap_pyfunction!(description, m)?)?;
    m.add_function(wrap_pyfunction!(file, m)?)?;
    m.add_function(wrap_pyfunction!(apptainer_config, m)?)?;
    m.add_function(wrap_pyfunction!(write_config_to_file, m)?)?;

    Ok(())
}
