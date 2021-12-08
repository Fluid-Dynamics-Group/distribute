use super::execute::FileMetadata;
use futures::StreamExt;
use std::path::{Path, PathBuf};

// clean out the tmp files from a build script from the output directory
// and recreate the distributed_save folder
pub(crate) async fn clean_output_dir(dir: &Path) -> Result<(), std::io::Error> {
    tokio::fs::remove_dir_all(dir).await.ok();

    tokio::fs::create_dir(dir).await?;
    tokio::fs::create_dir(dir.join("distribute_save")).await?;
    tokio::fs::create_dir(dir.join("input")).await?;
    tokio::fs::create_dir(dir.join("initial_files")).await?;

    Ok(())
}

// clear all the files from the distribute_save directory
pub(crate) async fn clean_distribute_save(base_path: &Path) -> Result<(), std::io::Error> {
    let dist_save = base_path.join("distribute_save");
    tokio::fs::remove_dir_all(&dist_save).await.ok();
    tokio::fs::create_dir(&dist_save).await?;

    Ok(())
}

pub(crate) async fn clear_input_files(base_path: &Path) -> Result<(), std::io::Error> {
    debug!("running clear_input_files for {}", base_path.display());
    let path = base_path.join("input");
    tokio::fs::remove_dir_all(&path).await?;
    tokio::fs::create_dir(&path).await?;

    debug!("removed and created ./input/ directory. Going to start copying form initial_files");

    let file_source = base_path.join("initial_files");

    let read_dir = tokio::fs::read_dir(&file_source).await?;
    let mut stream = tokio_stream::wrappers::ReadDirStream::new(read_dir);

    while let Some(file) = stream.next().await {
        if let Ok(file) = file {
            debug!(
                "handling the copying of file/ dir {}",
                file.path().display()
            );

            // read the file type
            if let Ok(file_type) = file.file_type().await {
                // check that it is a file
                if file_type.is_file() {
                    let from = file.path();
                    let to = path.join(file.file_name());

                    debug!("copying {} to {}", from.display(), to.display());
                    tokio::fs::copy(from, to).await?;
                } else {
                    continue;
                }
            } else {
                continue;
            }
        }
    }

    Ok(())
}

/// remove the directories up until "distribute_save" so that the file names that we send are
/// not absolute in their path - which makes saving things on the server side easier
pub(crate) fn remove_path_prefixes(path: PathBuf, distribute_save_path: &Path) -> PathBuf {
    path.strip_prefix(distribute_save_path).unwrap().to_owned()
}

pub(crate) async fn read_save_folder(base_path: &Path) -> Vec<FileMetadata> {
    walkdir::WalkDir::new(base_path.join("distribute_save"))
        .into_iter()
        .flat_map(|x| x.ok())
        .map(|x| FileMetadata {
            file_path: x.path().to_owned(),
            is_file: x.file_type().is_file(),
        })
        .collect()
}
