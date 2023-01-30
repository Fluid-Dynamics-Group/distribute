#![doc = include_str!("../README.md")]

mod wrap;

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use std::path::PathBuf;

use wrap::{ApptainerConfig, Description, File, Initialize, Job, Meta};

#[pyfunction(matrix_username = "None")]
/// construct the metadata ``Meta`` object for information about what this job batch will run
///
/// :param str namespace: namespace where several ``batch_name`` runs may live
/// :param str batch_name: the name of this batch of jobs
/// :param List[str] capabilities: required capabilities for your job
/// :param Optional[str] matrix_username: a matrix username in the format ``@your_username:homeserver_url``
///
/// for an apptainer job the ``capabilities`` are is simply ``["apptainer"]``. If you need GPU capabilities, use ``["apptainer", "gpu"]``.
///
/// Example:
///
/// .. code-block::
///
///     import distribute_compute_config as distribute
///     matrix_user = "@matrx_id:matrix.org"
///     batch_name = "test_batch"
///     namespace = "test_namespace"
///     capabilities = ["apptainer"]
///
///     meta = distribute.metadata(namespace, batch_name, capabilities, matrix_user)
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

    let meta = Meta {
        batch_name,
        namespace,
        capabilities,
        matrix,
    };
    Ok(meta)
}

#[pyfunction]
/// create the `initialize` section for loading ``apptainer`` ``.sif`` files, required container
/// mounts, and input files present in all runs.
///
/// Also see the documentation for creating a ``File`` with the :func:`file` function
///
/// :param sif_path: The path to the ``.sif`` file produced by ``apptainer build``
/// :param required_files: These files appear in the ``/input`` directory of every job, in addition to the ``File`` specified in each ``Job``
/// :param required_mounts: a list of strings to paths *inside* the container that should be mutable.
///
/// Example:
///
/// .. code-block::
///
///     import distribute_compute_config as distribute
///
///     sif_path = "./path/to/some/container.sif"
///
///     initial_condition = distribute.file(
///         "./path/to/some/file.h5",
///         relative=True,
///         alias="initial_condition.h5"
///     )
///     required_files = [initial_condition]
///
///     required_mounts = ["/solver/extra_mount"]
///
///     initialize = distribute.initialize(sif_path, required_files, required_mounts)
pub fn initialize(
    sif: File,
    required_files: Vec<File>,
    required_mounts: Vec<String>,
) -> Initialize {
    Initialize {
        sif,
        required_files,
        required_mounts: required_mounts.into_iter().map(PathBuf::from).collect(),
    }
}

#[pyfunction]
/// creates a job from its name and the required input files
///
/// once you have a list of jobs that should be run in the batch, move on to creating a
/// :func:`description()`
///
/// Also see the documentation for creating a ``File`` with the :func:`file()` function
///
/// :param name: the name of the job. It should be unique in combination with the ``batch_name`` of this job.
/// :param required_files: a python list of files that should appear in the ``/input`` directory when this job is run, along with the ``required_files`` specified in :func:`initialize()`.
///
/// ``name`` should therefore be unique to this batch since ``batch_name`` remains constant.
/// the ``name`` should be slightly descriptive of the content of what the job will be running.
/// This will make it easier to use ``distribute pull`` to download the files later
///
/// ``File`` types can be constructed with the :func:`file` function
///
/// Example:
///
/// .. code-block::
///
///     import distribute_compute_config as distribute
///
///     job_1_config_file = distribute.file(
///         "./path/to/config1.json",
///         alias="config.json",
///         relative=True
///     )
///     job_1_required_files = [job_1_config_file]
///
///     job_1 = distribute.job("job_1", job_1_required_files)
pub fn job(name: String, required_files: Vec<File>) -> Job {
    Job {
        name,
        required_files,
    }
}

#[pyfunction]
/// combines initialization information with a list of jobs to describe the solver files,
/// how they should be started, and what they will run
///
/// :param initialize: You can create an ``Initialize`` struct with :func:`initialize`
/// :param jobs: information for each job can be created from the :func:`job` function.
///
/// Example:
///
/// .. code-block::
///
///     import distribute_compute_config as distribute
///
///     initialize = distribute.initialize(
///         sif_path="./some/path/to/file.sif",
///         required_files=[],
///         required_mounts=[]
///     )
///
///     job_1_config_file = distribute.file(
///         "./path/to/config1.json",
///         alias="config.json",
///         relative=True
///     )
///
///     job_1 = distribute.job("job_1", [job_1_config_file])
///
///     description = distribute.description(initialize, jobs=[job_1])
pub fn description(initialize: Initialize, jobs: Vec<Job>) -> Description {
    Description { initialize, jobs }
}

#[pyfunction(relative = "false", alias = "None")]
/// create a file to appear in the ``/input`` directory of the solver
///
/// :param path: a path to the file on disk. ``path`` should be an absolute, unless ``relative=True`` is also specified. If ``relative=True`` is not speficied, the full directory structure to the file *must* already exist.
/// :param relative: a flag for if the ``path`` specified is a relative path or not. Defaults to `False`
/// :param alias: how the file should be renamed when it appears in the ``/input`` directory of your solver. If no ``alias`` is specified, the current name of the file will be used.
///
/// For example, a file with a ``path`` ``./path/to/config_1.json`` will appear as
/// ``/input/config_1.json``. with ``alias="config.json"``, this file will appear as
/// ``/input/config.json``
///
/// Example
///
///
/// .. code-block::
///
///     import distribute_compute_config as config
///
///     # config1.json appears as `/input/config.json`
///     config_file_1 = distribute.file(
///         "./path/to/config1.json",
///         alias="config.json",
///         relative=True
///     )
///
///     # config2.json appears as `/input/config2.json`
///     config_file_2 = distribute.file(
///         "./path/to/config2.json",
///         relative=True
///     )
///
///     # config3.json appears as `/input/config3.json`, folder structure
///     # `/root/path/to/` must already exist
///     config_file_3 = distribute.file("/root/path/to/config3.json")
pub fn file(path: PathBuf, relative: bool, alias: Option<String>) -> PyResult<File> {
    let file = match (relative, alias) {
        (true, Some(alias)) => {
            File::with_alias_relative(path, alias)
        }
        (false, Some(alias)) => {
            File::with_alias(path, alias).map_err(|e| PyValueError::new_err(format!("Failed to canonicalize the path provided. If this directory does not exist yet, you should use relative=True. Full error {e}")))?
        }
        (true, None) => {
            File::new_relative(path)
        }
        (false, None) => {
            File::new(path).map_err(|e| PyValueError::new_err(format!("Failed to canonicalize the path provided. If this directory does not exist yet, you should use relative=True. Full error {e}")))?
        }
    };

    Ok(file)
}

#[pyfunction]
/// assemble the :func:`metadata` and :func:`description` into a config file that can be written
/// to disk
///
/// :param meta: output of :func:`metadata` function
/// :param description: output of :func:`description` function
///
/// You can write this description object to disk with :func:`write_config_to_file`
pub fn apptainer_config(meta: Meta, description: Description) -> ApptainerConfig {
    ApptainerConfig { meta, description }
}

#[pyfunction]
/// write an :func:`apptainer_config()` to a path
///
/// :param config: output of :func:`apptainer_config()` function
/// :param path: the path that the config file should be written to, usually with the name ``distribute-jobs.yaml``
///
/// Example:
///
/// See the `User Documentation <https://fluid-dynamics-group.github.io/distribute-docs/python_api.html>`_ page in on the python api for a worked example
pub fn write_config_to_file(config: ApptainerConfig, path: PathBuf) -> PyResult<()> {
    let file = std::fs::File::create(&path).map_err(|e| {
        PyValueError::new_err(format!(
            "failed to create file at {}. Full error: {e}",
            path.display()
        ))
    })?;

    let distribute_config = config.into_distribute_version();

    distribute_config.to_writer(file).map_err(|e| {
        PyValueError::new_err(format!(
            "failed to serialize contents to file. Full error: {e}"
        ))
    })?;

    Ok(())
}

/// This python package provides compile-time checked functions to create ``distribute`` compute
/// configuration files. The recommended way to read this documentation is to start with the final
/// function :func:`write_config_to_file` and work backwards recursively until you have all the
/// constituent parts of the config file.
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
