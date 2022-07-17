use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use std::path::PathBuf;

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
    Ok(Meta(meta))
}

#[pyfunction]
fn initialize(
    sif_path: String,
    required_files: Vec<DistributeFile>,
    required_mounts: Vec<String>,
) -> PyResult<Initialize> {
    println!("!!!in initialize");
    let init = distribute::apptainer::Initialize::new(
        PathBuf::from(sif_path),
        required_files.into_iter().map(|x| x.file()).collect(),
        required_mounts.into_iter().map(PathBuf::from).collect(),
    );

    Ok(Initialize(init))
}

#[pyfunction]
fn job(name: String, required_files: Vec<DistributeFile>) -> PyResult<Job> {
    println!("!!!! start of job function call");
    let job =
        distribute::apptainer::Job::new(name, required_files.into_iter().map(|x| x.file()).collect());

    Ok(Job(job))
}

#[pyfunction]
fn description(initialize: Initialize, jobs: Vec<Job>) -> PyResult<ApptainerDescription> {
    let desc = distribute::apptainer::Description::new(initialize.0, jobs.into_iter().map(|x| x.0).collect());

    Ok(ApptainerDescription(desc))
}

#[pyfunction(relative="false", alias="None")]
fn file(path: PathBuf, relative: bool, alias: Option<String>) -> PyResult<DistributeFile> {
    
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

    Ok(DistributeFile::new(file))
}


#[pyclass]
#[derive(Clone)]
struct Meta(distribute::Meta);

#[pyclass]
#[derive(FromPyObject)]
struct Job(distribute::apptainer::Job);

#[pyclass]
#[derive(Clone)]
struct Initialize(distribute::apptainer::Initialize);

#[pyclass]
#[derive(FromPyObject)]
struct DistributeFile(distribute::common::File);

impl DistributeFile {
    fn new(x: distribute::common::File) -> Self {
        Self(x)
    }
    fn file(self) -> distribute::common::File {
        self.0
    }
}

fn extract_file(py: &PyAny) -> PyResult<distribute::common::File> {
    println!("!!! attempting to extract file!");
    Ok(py
        .extract::<distribute::common::File>()
        .expect("Could not cast"))
}

#[pyclass]
#[derive(Clone)]
struct ApptainerDescription(distribute::apptainer::Description);

#[pyclass]
struct ApptainerJobset {
    meta: Meta,
    description: ApptainerDescription,
}

#[pymethods]
impl ApptainerJobset {
    #[new]
    fn new(meta: Meta, description: ApptainerDescription) -> Self {
        Self{ meta, description }
    }

    fn write_to_path(&self, path: PathBuf) -> PyResult<()> {
        let config = distribute::Jobs::Apptainer { meta: self.meta.0.clone(), apptainer:self.description.0.clone() };

        let file = std::fs::File::create(&path)
            .map_err(|e| PyValueError::new_err(format!("failed to create distribute jobs file at {}, full error: {e}", path.display())))?;

        config.to_writer(file)
            .map_err(|e| PyValueError::new_err(format!("failed to serialize provided data to file. This should probably not happen. full error: {e}")))?;

        Ok(())
    }
}

#[pyfunction]
fn take_foo_wrapper(foo: FooWrapper) -> () {
    //
}

#[pyfunction]
fn take_foo(foo: Foo) -> () {
    //
}

#[pyfunction]
fn make_foo() -> Foo {
    Foo{ one: "made from foo".into()}
}

#[pyfunction]
fn make_foo_wrapper() -> FooWrapper {
    FooWrapper(Foo{ one: "made from wrapper".into()})
}


#[derive(Clone)]
#[pyclass]
struct FooWrapper(Foo);

#[derive(Clone)]
#[pyclass]
struct Foo {
    one: String,
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
    m.add_class::<DistributeFile>()?;
    m.add_class::<distribute::common::File>()?;

    m.add_function(wrap_pyfunction!(take_foo_wrapper, m)?)?;
    m.add_function(wrap_pyfunction!(take_foo, m)?)?;
    m.add_function(wrap_pyfunction!(make_foo, m)?)?;
    m.add_function(wrap_pyfunction!(make_foo_wrapper, m)?)?;
    Ok(())
}
