#![doc = include_str!("../README.md")]

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
/// construct the metadata `Meta` object for information about what this job batch will run
///
/// ## Arguments
///
/// ### `namespace`
///
/// the namespace is similar to the root directory where all your `batch_name`-named folders will
/// reside. For this reason, the `namespace` variable will probably remain constant for all config
/// files in a given project.
///
/// ### `batch_name`
///
/// the batch name is a description of all the [`job()`]s in the batch that you are running. 
///
/// Since completed batches are stored on disk at a directory `$namespace/$batch_name`, 
/// your `namespace` and `batch_name` combination must be unique. Since `namespace` likely remains
/// constant throughout the project, this implies `batch_name` must be unique for every batch ran.
///
/// ### `capabilities`
///
/// for an apptainer job, this is simply ["apptainer"]. If you need GPU capabilities, this this is
/// `["apptainer", "gpu"]`. 
///
/// ### `matrix_username`
///
/// a matrix username in the format `@your_username:homeserver_url`. An example user is
/// `"@karlik:matrix.org"`.
///
/// ## Example
///
/// in python:
///
/// ```python
/// import distribute_compute_config as distribute 
///
/// matrix_user = "@karik:matrix.org"
/// batch_name = "test_batch"
/// namespace = "test_namespace"
/// capabilities = ["apptainer"]
/// 
/// meta = distribute.metadata(namespace, batch_name, capabilities, matrix_user)
/// ```
pub fn metadata(
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
/// create the `initialize` section for loading `apptainer` `.sif` files, required container
/// mounts, and input files present in all runs.
///
/// Also see the documentation for creating a [`File`] with the [`file()`] function
///
/// ## Arguments
///
/// ### `sif_path`
///
/// The path to the `.sif` file produced by your `apptainer build`
///
/// ### `required_files`
///
/// These files appear in the `/input` directory of every job, in addition to the [`File`]'s
/// specified in each [`Job`]
///
/// ### `required_mounts`
///
/// a list of strings to paths *inside* the container that should be mutable.
///
/// ## Example
///
/// ```python
/// import distribute_compute_config as distribute 
///
/// sif_path = "./path/to/some/container.sif"
///
/// initial_condition = distribute.file("./path/to/some/file.h5", relative=True, alias="initial_condition.h5")
/// required_files = [initial_condition]
///
/// required_mounts = ["/solver/extra_mount"]
///
/// initialize = distribute.initialize(sif_path, required_files, required_mounts)
/// ```
pub fn initialize(
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
/// creates a job from its name and the required input files
///
/// once you have a list of jobs that should be run in the batch, move on to creating a
/// [`description()`]
///
/// Also see the documentation for creating a [`File`] with the [`file`](`file()`) function
///
/// ## Arguments
///
/// ### `name`
///
/// the name of the job. It should be unique in combination with the `batch_name` of this job,
/// implying uniqueness among all [`job()`] `name`s in this batch.
///
/// the job name should be slightly descriptive of the content of what the job will be running.
/// This will make it easier to use `distribute pull` to download the files later
///
/// ### `required_files` 
///
/// a python list of files that should appear in the `/input` directory when this job is run, along
/// with the `required_files` specified in [`initialize()`]. 
///
/// [`File`]s can be constructed with the [`file()`] function
///
/// ## Example
///
/// in python:
///
/// ```python
/// import distribute_compute_config as distribute
///
/// job_1_config_file = distribute.file("./path/to/config1.json", alias="config.json", relative=True)
/// job_1_required_files = [job_1_config_file]
///
/// job_1 = distribute.job("job_1", job_1_required_files)
/// ```
pub fn job(name: String, required_files: Vec<File>) -> Job {
    distribute::apptainer::Job::new(name, required_files)
}

#[pyfunction]
/// combines initialization information with a list of jobs to describe the solver files,
/// how they should be started, and what they will run
///
/// ## Variables
///
/// ### `initialize`
///
/// You can create an [`Initialize`] struct with [`initialize()`]
///
/// ### `jobs`
///
/// information for each job can be created from the [`job()`] function. `jobs` is simply a python
/// list of all job objects
///
/// ## Example
/// 
/// ```
/// import distribute_compute_config as distribute
///
/// initialize = distribute.initialize(sif_path="./some/path/to/file.sif", required_files=[], required_mounts=[])
///
/// job_1_config_file = distribute.file("./path/to/config1.json", alias="config.json", relative=True)
/// job_1 = distribute.job("job_1", [job_1_config_file])
///
/// description = distribute.description(initialize, jobs=[job_1])
/// ```
pub fn description(initialize: Initialize, jobs: Vec<Job>) -> Description {
    distribute::apptainer::Description::new(initialize, jobs)
}

#[pyfunction(relative="false", alias="None")]
/// create a file to appear in the `/input` directory of the solver
///
/// ## Arguments
///
/// ### `path`
///
/// a path to the file on disk. `path` should be an absolute, unless `relative=True` is also
/// specified. If `relative=True` is not speficied, the full directory structure to the file *must*
/// already exist.
///
/// ## Keyword Arguments
///
/// ### `relative`
///
/// a flag for if the `path` specified is a relative path or not. Defaults to `False`
///
/// ### `alias`
///
/// how the file should be renamed when it appears in the `/input` directory of your solver.
/// If no `alias` is specified, the current name of the file will be used. 
///
/// For example, a file with a `path` `./path/to/config_1.json` will appear as
/// `/input/config_1.json`. with `alias="config.json"`, this file will appear as
/// `/input/config.json`
///
/// defaults to `None`
///
/// ## Example
///
/// in python:
///
/// ```python
/// import distribute_compute_config as config
///
/// ## config1.json appears as `/input/config.json`
/// config_file_1 = distribute.file("./path/to/config1.json", alias="config.json", relative=True)
/// ## config2.json appears as `/input/config2.json`
/// config_file_2 = distribute.file("./path/to/config2.json", relative=True)
///
/// ## config3.json appears as `/input/config3.json`, folder structure `/root/path/to/` must already
/// ## exist
/// config_file_3 = distribute.file("/root/path/to/config3.json")
/// ```
pub fn file(path: PathBuf, relative: bool, alias: Option<String>) -> PyResult<distribute::common::File> {
    
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
/// assemble the [`metadata()`] and [`description()`] into a config file that can be written
/// to disk
///
/// ## Arguments
///
/// ### `meta`
///
/// output of [`metadata()`] function
///
/// ### `description`
///
/// output of [`description()`] function
///
/// ## Example
///
/// See the [user documentation](https://fluid-dynamics-group.github.io/distribute-docs/python_api.html/) page in on the python api for a worked example
pub fn apptainer_config(meta: Meta, description: Description) -> ApptainerConfig {
    ApptainerConfig::new(meta, description)
}

#[pyfunction]
/// write an [`apptainer_config()`] to a path
///
/// ## Arguments
///
/// ### `config`
///
/// output of [`apptainer_config()`] function
///
/// ### `path`
///
/// the path that the config file should be written to, usually with the name
/// `distribute-jobs.yaml`
///
/// ## Example
///
/// See the [user documentation](https://fluid-dynamics-group.github.io/distribute-docs/python_api.html/) page in on the python api for a worked example
pub fn write_config_to_file(config: ApptainerConfig, path: PathBuf) -> PyResult<()> {
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
fn distribute_compute_config(_py: Python<'_>, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(metadata, m)?)?;
    m.add_function(wrap_pyfunction!(initialize, m)?)?;
    m.add_function(wrap_pyfunction!(job, m)?)?;
    m.add_function(wrap_pyfunction!(description, m)?)?;
    m.add_function(wrap_pyfunction!(file, m)?)?;
    m.add_function(wrap_pyfunction!(apptainer_config, m)?)?;
    m.add_function(wrap_pyfunction!(write_config_to_file, m)?)?;

    Ok(())
}
