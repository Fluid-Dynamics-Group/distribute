use super::pool_data::{CancelBatchQuery, RemainingJobsQuery};
use super::JobRequest;

use crate::prelude::*;

use walkdir::{DirEntry, WalkDir};

use config::NormalizePaths;

/// handle incomming requests from the user over CLI on any node
pub(crate) async fn handle_user_requests(
    port: u16,
    tx: mpsc::Sender<JobRequest>,
    node_capabilities: Vec<Arc<Requirements<NodeProvidedCaps>>>,
    results_directory: PathBuf,
    job_input_file_dir: PathBuf,
) {
    debug!("binding to listener");

    let listener = TcpListener::bind(SocketAddr::from(([0, 0, 0, 0], port)))
        .await
        .map_err(error::TcpConnection::from)
        .unwrap();

    debug!("finished binding to port");

    loop {
        let (tcp_conn, address) = listener
            .accept()
            .await
            .map_err(error::TcpConnection::from)
            .unwrap();

        info!("new user connection from {}", address);

        let conn =
            transport::Connection::<transport::ServerResponseToUser>::from_connection(tcp_conn);

        let tx_c = tx.clone();
        let node_c = node_capabilities.clone();

        let results_directory_own = results_directory.to_owned();
        let job_input_file_dir_own = job_input_file_dir.to_owned();

        tokio::spawn(async move {
            single_user_request(
                conn,
                tx_c,
                node_c,
                results_directory_own,
                job_input_file_dir_own,
            )
            .await;
            info!("user connection has closed");
        });
    }
}

// TODO: add user message to end the connection
// so that this function does not hang forever and generate extra tasks
async fn single_user_request(
    mut conn: transport::Connection<transport::ServerResponseToUser>,
    tx: mpsc::Sender<JobRequest>,
    node_capabilities: Vec<Arc<Requirements<NodeProvidedCaps>>>,
    results_directory: PathBuf,
    job_input_file_dir: PathBuf,
) {
    loop {
        let request = match conn.receive_data().await {
            Ok(req) => {
                info!("a user request has been received");
                req
            }
            Err(e) => {
                match e {
                    error::TcpConnection::ConnectionClosed => continue,
                    x => {
                        error!("error reading user request: {}", x);
                    }
                }
                return;
            }
        };

        match request {
            transport::UserMessageToServer::AddJobSet(set) => {
                let mut set: config::Jobs<_> = set.into();
                // normalize all the paths in the configuration to use the base path of
                // the storage location that they were stored in earlier
                set.normalize_paths(job_input_file_dir.clone());

                dbg!(&set);

                conn = add_job_set(&tx, set, conn, &job_input_file_dir).await;
                debug!("the new request was AddJobSet");
            }
            transport::UserMessageToServer::QueryCapabilities => {
                debug!("the new request was QueryCapabilities");
                query_capabilities(&mut conn, &node_capabilities).await;
            }
            transport::UserMessageToServer::QueryJobNames => {
                debug!("the new request was to query the job names");
                query_job_names(&tx, &mut conn).await;
            }
            transport::UserMessageToServer::KillJob(batch_name) => {
                debug!("new request was to kill a job set");
                // TODO: ask the scheduler
                cancel_job_by_name(&tx, &mut conn, batch_name).await;
            }
            transport::UserMessageToServer::PullFilesInitialize(req) => {
                debug!("new request to pull files from a batch");
                // TODO: ask the scheduler
                pull_files(&results_directory, &mut conn, req).await;
            }
            transport::UserMessageToServer::FileReceived => {
                warn!(
                    "got FileReceived signal from user connection in main handling 
                      process. This should not happen! Ignoring for now"
                );
            }
        }
        //
    }
}

async fn add_job_set(
    tx: &mpsc::Sender<JobRequest>,
    set: config::Jobs<config::common::HashedFile>,
    mut conn: transport::Connection<transport::ServerResponseToUser>,
    distribute_file_save_location: &Path,
) -> transport::Connection<transport::ServerResponseToUser> {
    if let Err(e) = conn
        .transport_data(&transport::ServerResponseToUser::Continue)
        .await
    {
        error!("failed to notifiy user to continue sending init files: {e}");
        return conn;
    }

    // the connection is now in a state to receive files
    let conn = conn.update_state();

    // need some of these channels since the send_files state machine currently expects
    // to tell the scheduler at the end that it has finished the job and we have not
    // broken that portion of the code into its own state machine yet
    let (mut empty_tx, _empty_rx) = mpsc::channel(5);

    //
    // receive all the files from the user
    //

    let extra = protocol::send_files::Nothing::new();
    let state = protocol::send_files::ReceiverState {
        conn,
        save_location: distribute_file_save_location.to_path_buf(),
        extra,
    };

    let machine = protocol::Machine::from_state(state);
    let conn = match machine.receive_files(&mut empty_tx).await {
        Ok(conn) => conn.into_inner(),
        Err((machine, e)) => {
            error!(
                "error receiving a file from the user when adding a job set: {}",
                e
            );
            return machine.into_connection().update_state();
        }
    };

    // convert the connection into a user-facing connection again
    let mut conn = conn.update_state();

    //
    // add the job set to the scheduler
    //
    let tx_result = tx.send(JobRequest::AddJobSet(set)).await.map_err(|e| {
        error!(
            "error sending job set to pool (this should not happen): {}",
            e
        );
    });

    // if we were able to add the job set to the scheduler
    if tx_result.is_ok() {
        if let Err(e) = conn
            .transport_data(&transport::ServerResponseToUser::JobSetAdded)
            .await
        {
            error!(
                "could not respond to the user that the job set was added: {}",
                e
            );
        } else {
            debug!("alerted the client that the job set was added");
        }
    }
    // we were NOT able to schedule the job
    else {
        debug!("job set was successfully added to the server");

        conn.transport_data(&transport::ServerResponseToUser::JobSetAddedFailed)
            .await
            // we told the  user about it
            .map(|_| debug!("alterted that the client that the job set could not be added"))
            // we errored when trying to tell the user about it
            // this should probably not happen unless the client aborted the connection
            .map_err(|e| {
                error!(
                    "could not alert the client that the job was added successfully: {}",
                    e
                )
            })
            .ok();
    }

    return conn;
}

async fn query_capabilities(
    conn: &mut transport::Connection<transport::ServerResponseToUser>,
    node_capabilities: &[Arc<Requirements<NodeProvidedCaps>>],
) {
    // clone all the data so that we have non-Arc'd data
    // this can be circumvented by
    let caps: Vec<Requirements<_>> = node_capabilities.iter().map(|x| (**x).clone()).collect();

    if let Err(e) = conn
        .transport_data(&transport::ServerResponseToUser::Capabilities(caps))
        .await
    {
        error!("error sending caps to user (this should not happen): {}", e)
    }
}

async fn query_job_names(
    tx: &mpsc::Sender<JobRequest>,
    conn: &mut transport::Connection<transport::ServerResponseToUser>,
) {
    let (tx_respond, rx_respond) = oneshot::channel();

    if let Err(e) = tx
        .send(JobRequest::QueryRemainingJobs(RemainingJobsQuery::new(
            tx_respond,
        )))
        .await
    {
        error!(
            "could not send message to job pool for `query_job_names`. This should not happen. {}",
            e
        );

        let response = transport::ServerResponseToUser::JobNamesFailed;
        if let Err(e) = conn.transport_data(&response).await {
            error!(
                "failed to respond to the user with failure to query job names: {}",
                e
            );
        }
    }

    match rx_respond.await {
        Ok(remaining_jobs) => {
            let response = transport::ServerResponseToUser::JobNames(remaining_jobs);

            if let Err(e) = conn.transport_data(&response).await {
                error!("failed to respond to the user with the job names: {}", e);
            }
        }
        Err(e) => {
            error!(
                "job pool did not respond over the oneshot channel. This should not happen: {:?}",
                e
            );

            let response = transport::ServerResponseToUser::JobNamesFailed;

            if let Err(e) = conn.transport_data(&response).await {
                error!("failed to respond to the user with failure to query job names (caused by oneshot channel): {}", e);
            }
        }
    }
}

async fn cancel_job_by_name(
    tx: &mpsc::Sender<JobRequest>,
    conn: &mut transport::Connection<transport::ServerResponseToUser>,
    batch: String,
) {
    let (tx_respond, rx_respond) = oneshot::channel();

    let req = JobRequest::CancelBatchByName(CancelBatchQuery::new(tx_respond, batch.clone()));

    if let Err(e) = tx.send(req).await {
        error!(
            "could not query job pool to remove a job set {} - {}",
            batch, e
        );
        let response = transport::ServerResponseToUser::KillJobFailed;
        conn.transport_data(&response).await.ok();
    }

    match rx_respond.await {
        Ok(result) => {
            let resp = transport::ServerResponseToUser::KillJob(result);
            conn.transport_data(&resp).await.ok();
        }
        Err(_e) => {
            error!("could not read from oneshot pipe when getting killed job result");
            let response = transport::ServerResponseToUser::KillJobFailed;
            conn.transport_data(&response).await.ok();
        }
    }
}

async fn pull_files(
    folder_path: &Path,
    conn: &mut transport::Connection<transport::ServerResponseToUser>,
    pull_files: transport::PullFileRequest,
) {
    let namespace_path = folder_path.join(pull_files.namespace);
    info!("pulling files at path {}", namespace_path.display());

    if !namespace_path.exists() {
        conn.transport_data(&error::PullError::MissingNamespace.into())
            .await
            .ok();
        return;
    }

    let batch_path = namespace_path.join(pull_files.batch_name);

    if !batch_path.exists() {
        conn.transport_data(&error::PullError::MissingBatchname.into())
            .await
            .ok();
        return;
    }

    // all the filters should be checked client side,
    // so we omit checking them here
    let filters = pull_files
        .filters
        .into_iter()
        .filter_map(|x| regex::Regex::new(&x).ok())
        .collect();

    let walk_dir = WalkDir::new(&batch_path);
    let files = filter_files(
        walk_dir.into_iter(),
        filters,
        pull_files.is_include_filter,
        namespace_path,
        pull_files.skip_folders,
    );

    // if we are executing a dry response and only sending the names of the
    // files that matched and didnt match then
    // we dont need to pull the actual data from the files
    if pull_files.dry {
        let mut matched = Vec::new();
        let mut filtered = Vec::new();

        files.for_each(|x| match x {
            FilterResult::Include { abs: _, rel } => matched.push(rel),
            FilterResult::Skip { abs: _, rel } => filtered.push(rel),
        });

        let ret = transport::PullFilesDryResponse::new(matched, filtered);
        conn.transport_data(&ret.into()).await.ok();
    }
    // otherwise, we load each and every single file that we have parsed and prepare them
    // to be sent to the client
    else {
        let send_files = files.filter_map(|x| match x {
            FilterResult::Include { abs, rel } => Some((abs, rel)),
            _ => None,
        });

        for (abs_path, relative_path) in send_files {
            debug!(
                "sending file to user at abs path: `{}` rel path `{}`",
                abs_path.display(),
                relative_path.display()
            );

            if abs_path.is_dir() {
                // we are sending a directory
                let send_file = transport::SendFile::new(relative_path, false, vec![]);
                if let Err(e) = conn.transport_data(&send_file.into()).await {
                    error!("failed to transport directory through connection: {}", e);
                }
            } else {
                let file_length = std::fs::metadata(&abs_path)
                    .map(|meta| meta.len())
                    .map_err(|_| {
                        warn!(
                            "failed to read metadata for {} - defaulting to 0 length",
                            abs_path.display()
                        )
                    })
                    .unwrap_or(0);

                trace!(
                    "file length for {} is {} bytes",
                    relative_path.display(),
                    file_length
                );

                // sending a large file
                if file_length > 10u64.pow(9) {
                    if let Err(e) =
                        send_large_file(abs_path, relative_path, file_length, conn).await
                    {
                        error!("failed to send large file: {}", e);
                    }
                }
                // sending a regular file
                else if let Err(e) = send_regular_file(abs_path, relative_path, conn).await {
                    error!("failed to send regular file: {}", e);
                }
            };

            match conn.receive_data().await {
                Ok(transport::UserMessageToServer::FileReceived) => continue,
                Err(error::TcpConnection::ConnectionClosed) => {
                    warn!("TCP connection has closed - severing the connection to the user");
                    break;
                }
                other => {
                    warn!("user response from file was {:?} which was unexpected - closing connection", other);
                }
            }
        }

        conn.transport_data(&transport::ServerResponseToUser::FinishFiles)
            .await
            .ok();
    }
}

async fn send_regular_file(
    absolute_path: PathBuf,
    relative_path: PathBuf,
    conn: &mut transport::Connection<transport::ServerResponseToUser>,
) -> Result<(), Box<dyn std::error::Error>> {
    // we are sending a file
    match std::fs::read(&absolute_path) {
        Ok(bytes) => {
            debug!(
                "size of buffer after reading file {} capacity: {}",
                bytes.len(),
                bytes.capacity()
            );

            let send_file = transport::SendFile::new(relative_path, true, bytes);
            conn.transport_data(&send_file.into()).await.ok();
        }
        Err(e) => {
            // send an error message for this file
            let msg = error::PullError::LoadFile(absolute_path.clone());
            conn.transport_data(&msg.into()).await.ok();
            return Err(error::ReadBytes::new(e, absolute_path).into());
        }
    };

    Ok(())
}

async fn send_large_file(
    absolute_path: PathBuf,
    relative_path: PathBuf,
    file_size: u64,
    conn: &mut transport::Connection<transport::ServerResponseToUser>,
) -> Result<(), Box<dyn std::error::Error>> {
    // first, tell the client that we are sending a large file
    let marker = transport::FileMarker::new(relative_path);
    conn.transport_data(&transport::ServerResponseToUser::FileMarker(marker))
        .await?;

    // await a response from them - it will tell us they have received the marker
    conn.receive_data().await?;

    // set up a reader for the file
    // TODO: if returning early here it will mess with the expected results for the other side
    let reader = tokio::fs::File::open(&absolute_path)
        .await
        .map_err(|e| error::ReadBytes::new(e, absolute_path))?;

    conn.transport_from_reader(reader, file_size).await?;

    Ok(())
}

fn filter_files(
    dir_iter: impl Iterator<Item = Result<DirEntry, walkdir::Error>>,
    filters: Vec<regex::Regex>,
    is_include_filter: bool,
    prefix_to_strip: PathBuf,
    skip_folders: bool,
) -> impl Iterator<Item = FilterResult> {
    dir_iter.filter_map(|x| x.ok()).map(move |x| {
        // if the file is a directory and we are skipping directories, just skip it
        if skip_folders && x.file_type().is_dir() {
            FilterResult::skip(x.path().to_owned(), &prefix_to_strip)
        }
        // always make sure that we include directories
        else if x.file_type().is_dir() && !skip_folders {
            FilterResult::include(x.path().to_owned(), &prefix_to_strip)
        }
        // if the file is not a directory - cycle to make sure that we match on the regular
        // expressions
        else {
            filter_path(
                filters.iter(),
                is_include_filter,
                x.path(),
                &prefix_to_strip,
            )
        }
    })
}

#[derive(Debug)]
enum FilterResult {
    Skip { abs: PathBuf, rel: PathBuf },
    Include { abs: PathBuf, rel: PathBuf },
}

impl FilterResult {
    fn skip(abs: PathBuf, prefix_to_strip: &Path) -> Self {
        Self::Skip {
            rel: abs.strip_prefix(prefix_to_strip).unwrap().to_owned(),
            abs,
        }
    }

    fn include(abs: PathBuf, prefix_to_strip: &Path) -> Self {
        Self::Include {
            rel: abs.strip_prefix(prefix_to_strip).unwrap().to_owned(),
            abs,
        }
    }
}

/// helper function to help execute a list of regular expressions on a single path
fn filter_path<'a>(
    filters: impl Iterator<Item = &'a regex::Regex>,
    is_include_filter: bool,
    path: &Path,
    prefix_to_strip: &Path,
) -> FilterResult {
    for expr in filters {
        // if we have a match to the expression
        if expr.find(&path.to_string_lossy()).is_some() {
            if is_include_filter {
                return FilterResult::include(path.to_owned(), prefix_to_strip);
            } else {
                return FilterResult::skip(path.to_owned(), prefix_to_strip);
            }
        }
    }

    // if we have gotten here then we need to find out what
    // we do if the regular expressions did not match. If we required that the regular expressions
    // should have matched the files, then we skip the file here
    if is_include_filter {
        FilterResult::skip(path.to_owned(), prefix_to_strip)
    } else {
        FilterResult::include(path.to_owned(), prefix_to_strip)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIR: &str = "./tests/filter_files";
    const DIR_PREFIX: &str = "./tests/";

    fn check_files(
        actual_files: Vec<FilterResult>,
        expected_included_items: Vec<PathBuf>,
        expected_removed_items: Vec<PathBuf>,
    ) {
        dbg!(&actual_files);

        for file in actual_files {
            match file {
                FilterResult::Skip { abs: _, rel } => {
                    if expected_included_items.contains(&rel) {
                        panic!(
                            "item {} was skipped, but we expected that it would be included",
                            rel.display()
                        );
                    }

                    if !expected_removed_items.contains(&rel) {
                        panic!(
                            "item {} was not marked to be skipped, but it was",
                            rel.display()
                        );
                    }
                }
                FilterResult::Include { abs: _, rel } => {
                    if expected_removed_items.contains(&rel) {
                        panic!(
                            "item {} was marked to be skipped, but it was included",
                            rel.display()
                        );
                    }

                    if !expected_included_items.contains(&rel) {
                        panic!(
                            "item {} was included, but it was not marked to be included",
                            rel.display()
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn include_folders() {
        let dir = PathBuf::from(DIR);
        let dir_prefix = PathBuf::from(DIR_PREFIX);

        let filters = vec![];
        // make it an exclude filter so that the filters do not
        // actually remove any files. If we did use an include filter then
        // we would have to write regexes to ensure all files are pulled.
        let is_include_filter = false;
        let skip_folders = false;

        let walk_dir = WalkDir::new(&dir);
        let files = filter_files(
            walk_dir.into_iter(),
            filters,
            is_include_filter,
            dir_prefix,
            skip_folders,
        )
        .collect::<Vec<_>>();

        dbg!(&files);

        let expected_included_items: Vec<PathBuf> = [
            "filter_files/",
            "filter_files/README.md",
            "filter_files/nested_folder/",
            "filter_files/nested_folder/another_file.txt",
        ]
        .into_iter()
        .map(PathBuf::from)
        .collect::<Vec<_>>();

        let expected_removed_items: Vec<PathBuf> = vec![];

        check_files(files, expected_included_items, expected_removed_items)
    }

    #[test]
    fn skip_folders() {
        let dir = PathBuf::from(DIR);
        let dir_prefix = PathBuf::from(DIR_PREFIX);

        let filters = vec![];
        // make it an exclude filter so that the filters do not
        // actually remove any files. If we did use an include filter then
        // we would have to write regexes to ensure all files are pulled.
        let is_include_filter = false;
        let skip_folders = true;

        let walk_dir = WalkDir::new(&dir);
        let files = filter_files(
            walk_dir.into_iter(),
            filters,
            is_include_filter,
            dir_prefix,
            skip_folders,
        )
        .collect::<Vec<_>>();

        dbg!(&files);

        let expected_included_items: Vec<PathBuf> = [
            "filter_files/README.md",
            "filter_files/nested_folder/another_file.txt",
        ]
        .into_iter()
        .map(PathBuf::from)
        .collect::<Vec<_>>();

        let expected_removed_items: Vec<PathBuf> = ["filter_files/", "filter_files/nested_folder/"]
            .into_iter()
            .map(PathBuf::from)
            .collect::<Vec<_>>();

        check_files(files, expected_included_items, expected_removed_items)
    }
}
