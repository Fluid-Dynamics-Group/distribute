use super::CanonicalizeError;
use super::LoadJobsError;
use super::MissingFileNameError;
use super::ReadBytesError;

use serde::{Deserialize, Serialize};
use std::path::Path;
use std::path::PathBuf;

#[cfg(feature = "cli")]
use crate::transport;

use derive_more::Constructor;

use super::NormalizePaths;

#[derive(Debug, Clone, Deserialize, Serialize, getset::Getters)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "python", pyo3::pyclass)]
pub struct File {
    // the path to the file locally
    #[getset(get = "pub(crate)")]
    path: PathBuf,
    // the save name of the file in the root
    // directory once it has been transported to the client
    alias: Option<String>,
}

impl File {
    pub fn with_alias<T: Into<PathBuf>, U: Into<String>>(
        path: T,
        alias: U,
    ) -> Result<Self, LoadJobsError> {
        let path = path.into();
        let path = path
            .canonicalize()
            .map_err(|e| CanonicalizeError::new(path, e))?;
        Ok(Self {
            path,
            alias: Some(alias.into()),
        })
    }

    /// create a `File` without an alias and dont try to canonicalize the path to something real
    ///
    /// most useful for templating the results - when the path does not actually exist
    pub fn with_alias_relative<T: Into<PathBuf>, U: Into<String>>(path: T, alias: U) -> Self {
        let path = path.into();
        Self {
            path,
            alias: Some(alias.into()),
        }
    }

    /// create a new `File` and ensure that the path is an absolute path
    pub fn new<T: Into<PathBuf>>(path: T) -> Result<Self, LoadJobsError> {
        let path = path.into();
        let path = path
            .canonicalize()
            .map_err(|e| CanonicalizeError::new(path, e))?;
        Ok(Self { path, alias: None })
    }

    /// create a file without an alias, and dont try to canonicalize the path to something real
    ///
    /// this is most useful for templating the results
    pub fn new_relative<T: Into<PathBuf>>(path: T) -> Self {
        let path = path.into();
        Self { path, alias: None }
    }

    pub fn alias(&self) -> Option<&String> {
        self.alias.as_ref()
    }
    pub(crate) fn filename(&self) -> Result<String, MissingFileNameError> {
        if let Some(alias) = &self.alias {
            Ok(alias.to_string())
        } else {
            let out = self
                .path
                .file_name()
                .ok_or_else(|| MissingFileNameError::from(self.path.clone()))?
                .to_string_lossy()
                .to_string();
            Ok(out)
        }
    }

    pub(crate) fn hashed_file(&mut self, hash: String) -> Result<(), LoadJobsError> {
        let filename = self.filename()?;

        self.alias = Some(filename);
        self.path = PathBuf::from(hash);

        Ok(())
    }
}

impl NormalizePaths for File {
    fn normalize_paths(&mut self, base: PathBuf) {
        self.path = normalize_pathbuf(self.path.clone(), base);
    }
}

#[cfg(feature = "cli")]
pub(crate) async fn load_from_file(files: &[File]) -> Result<Vec<transport::File>, LoadJobsError> {
    let mut job_files = vec![];

    for file in files.iter() {
        let file_bytes = tokio::fs::read(&file.path)
            .await
            .map_err(|e| LoadJobsError::from(ReadBytesError::new(e, file.path.clone())))?;

        let file_name = file.filename()?;

        job_files.push(transport::File {
            file_name,
            file_bytes,
        });
    }

    Ok(job_files)
}

pub(crate) fn normalize_pathbuf(pathbuf: PathBuf, base_path: PathBuf) -> PathBuf {
    if pathbuf.is_relative() {
        base_path.join(pathbuf)
    } else {
        pathbuf
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, getset::Getters, Constructor)]
pub struct HashedFile {
    // the path to the file locally
    #[getset(get = "pub(crate)")]
    hashed_filename: String,
    //
    path: PathBuf,
}

#[cfg(feature = "cli")]
impl HashedFile {
    pub(crate) fn lazy_file_unchecked(&self) -> crate::server::LazyFile {
        crate::server::LazyFile::new(self.hashed_filename.clone(), self.path.clone())
    }
}

impl NormalizePaths for HashedFile {
    fn normalize_paths(&mut self, base: PathBuf) {
        self.path = normalize_pathbuf(self.path.clone(), base);
    }
}
