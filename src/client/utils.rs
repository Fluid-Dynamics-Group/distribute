use std::path::{Path, PathBuf};

// clean out the tmp files from a build script from the output directory
// and recreate the distributed_save folder
pub(crate) async fn clean_output_dir(dir: &Path) -> Result<(), std::io::Error> {
    tokio::fs::remove_dir_all(dir).await.ok();

    tokio::fs::create_dir(dir).await?;
    tokio::fs::create_dir(dir.join("distribute_save")).await?;

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
