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

pub(crate) async fn clear_input_files(base_path: &Path) -> Result<(), std::io::Error> {
    let path = base_path.join("input");
    tokio::fs::remove_dir_all(&path).await?;
    tokio::fs::create_dir(&path).await?;

    let file_source = base_path.join("initial_files");

    let read_dir = tokio::fs::read_dir(&file_source).await?;
    let mut stream = tokio_stream::wrappers::ReadDirStream::new(read_dir);

    while let Some(file) = stream.next().await {
        if let Ok(file) = file {
            // read the file type
            if let Ok(file_type) = file.file_type().await {
                // check that it is a file
                if file_type.is_file() {
                    tokio::fs::copy(file_source.join(file.path()), path.join(file.path())).await;
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
pub(crate) fn remove_path_prefixes(path: PathBuf) -> PathBuf {
    path.components()
        .skip_while(|x| x.as_os_str() == "distribute_save")
        .skip(2)
        .collect()
}
