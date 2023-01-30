use super::CanonicalizeError;
use super::LoadJobsError;
use super::MissingFileNameError;
use super::ReadBytesError;

use serde::{Deserialize, Serialize};

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

    pub(super) fn exists_or_err(&self) -> Result<(), super::MissingFileNameError> {
        if !self.path().exists() {
            return Err(super::MissingFileNameError::new(self.path().to_owned()));
        }
        Ok(())
    }

    #[cfg(test)]
    /// get a quick and dirty conversion to a hashed file
    pub(super) fn hashed(&self) -> Result<HashedFile, super::MissingFileNameError> {
        let hashed_path = self.path().to_path_buf();
        let unhashed_path_user = self.path().to_path_buf();
        let original_filename = self.filename()?;

        Ok(HashedFile {
            hashed_path,
            unhashed_path_user,
            original_filename,
        })
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
    // on the server, this is the absolute path to the file on disk
    //
    // on the user's computer, this is a relative path that only contains the hash of the file
    // so that the file will be dumped precisely where we intend it to be dumped on the server
    // through send_files
    hashed_path: PathBuf,
    // on the user's computer, this is the absolute path to the file
    //
    // on the server, this is not really used
    unhashed_path_user: PathBuf,
    // the path to the file locally
    #[getset(get = "pub(crate)")]
    original_filename: String,
}

#[cfg(feature = "cli")]
impl HashedFile {
    pub(crate) fn as_sendable(&self, is_user: bool) -> crate::client::execute::FileMetadata {
        if is_user {
            self.as_sendable_user()
        } else {
            self.as_sendable_server()
        }
    }

    pub(crate) fn as_sendable_user(&self) -> crate::client::execute::FileMetadata {
        crate::client::execute::FileMetadata::file(
            // location on the disk here
            &self.unhashed_path_user,
            // location we intend to send this file to
            &self.hashed_path,
        )
    }

    pub(crate) fn as_sendable_server(&self) -> crate::client::execute::FileMetadata {
        crate::client::execute::FileMetadata::file(
            // location on the disk here
            self.hashed_path.clone(),
            // location we intend to send this to
            PathBuf::from(self.original_filename.clone()),
        )
    }

    pub(super) fn delete_at_hashed_path(&self) -> Result<(), std::io::Error> {
        std::fs::remove_file(&self.hashed_path)?;

        Ok(())
    }
}

impl NormalizePaths for HashedFile {
    fn normalize_paths(&mut self, base: PathBuf) {
        self.hashed_path = normalize_pathbuf(self.hashed_path.clone(), base);
    }
}

#[cfg(feature = "cli")]
pub(super) fn delete_hashed_files(files: Vec<HashedFile>) -> Result<(), std::io::Error> {
    for file in files {
        file.delete_at_hashed_path()?;
    }

    Ok(())
}

pub(super) fn check_files_exist(files: &[File]) -> Result<(), super::MissingFileNameError> {
    for file in files {
        file.exists_or_err()?;
    }

    Ok(())
}
