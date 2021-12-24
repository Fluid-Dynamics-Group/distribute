use super::pool_data::{CancelBatchQuery, RemainingJobsQuery};
use crate::{error, transport};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use std::sync::Arc;

use tokio::net::TcpListener;
use tokio::sync::{mpsc, oneshot};

use super::{JobRequest, NodeProvidedCaps, Requirements};
use std::io;
use walkdir::{DirEntry, WalkDir};

/// handle incomming requests from the user over CLI on any node
pub(crate) async fn handle_user_requests(
    port: u16,
    tx: mpsc::Sender<JobRequest>,
    node_capabilities: Vec<Arc<Requirements<NodeProvidedCaps>>>,
    path: PathBuf,
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

        let conn = transport::ServerConnectionToUser::new(tcp_conn);

        let tx_c = tx.clone();
        let node_c = node_capabilities.clone();

        let path_own = path.to_owned();

        tokio::spawn(async move {
            single_user_request(conn, tx_c, node_c, path_own).await;
            info!("user connection has closed");
        });
    }
}

// TODO: add user message to end the connection
// so that this function does not hang forever and generate extra tasks
async fn single_user_request(
    mut conn: transport::ServerConnectionToUser,
    tx: mpsc::Sender<JobRequest>,
    node_capabilities: Vec<Arc<Requirements<NodeProvidedCaps>>>,
    path: PathBuf,
) {
    loop {
        let request = match conn.receive_data().await {
            Ok(req) => {
                info!("a user request has been received");
                req
            }
            Err(e) => {
                error!("error reading user request: {}", e);
                return;
            }
        };

        match request {
            transport::UserMessageToServer::AddJobSet(set) => {
                add_job_set(&tx, set, &mut conn).await;
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
                pull_files(&path, &mut conn, req).await;
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
    set: super::OwnedJobSet,
    conn: &mut transport::ServerConnectionToUser,
) {
    // the output of sending the message to the server
    let tx_result = tx.send(JobRequest::AddJobSet(set)).await.map_err(|e| {
        error!(
            "error sending job set to pool (this should not happen): {}",
            e
        );
    });

    // if we were able to add the job set to the scheduler
    if let Ok(_) = tx_result {
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
}

async fn query_capabilities(
    conn: &mut transport::ServerConnectionToUser,
    node_capabilities: &Vec<Arc<Requirements<NodeProvidedCaps>>>,
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
    conn: &mut transport::ServerConnectionToUser,
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
    conn: &mut transport::ServerConnectionToUser,
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
    conn: &mut transport::ServerConnectionToUser,
    pull_files: transport::PullFileRequest,
) {
    let namespace_path = folder_path.join(pull_files.namespace);
    info!("pulling files at path {}", namespace_path.display());

    if !namespace_path.exists() {
        conn.transport_data(&error::PullError::MissingNamespace.into())
            .await
            .ok();
        return ();
    }

    let batch_path = namespace_path.join(pull_files.batch_name);

    if !batch_path.exists() {
        conn.transport_data(&error::PullError::MissingBatchname.into())
            .await
            .ok();
        return ();
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

            let send_file = if abs_path.is_dir() {
                transport::SendFile::new(relative_path, false, vec![])
            } else {
                if let Ok(bytes) = std::fs::read(&abs_path) {
                    transport::SendFile::new(relative_path, true, bytes)
                } else {
                    // send an error message for this file
                    let msg = error::PullError::LoadFile(abs_path);
                    conn.transport_data(&msg.into()).await.ok();
                    continue;
                }
            };

            conn.transport_data(&send_file.into()).await.ok();

            match conn.receive_data().await {
                Ok(transport::UserMessageToServer::FileReceived) => continue,
                Err(error::Error::TcpConnection(error::TcpConnection::ConnectionClosed)) => {
                    warn!("TCP connection has closed - severing the connection to the user");
                    break;
                }
                other => {
                    warn!("user response from file was {:?} which was unexpected - closing connection", other);
                }
            }
        }

        conn.transport_data(&transport::ServerResponseToUser::FinishFiles.into())
            .await
            .ok();
    }
}

fn filter_files(
    dir_iter: impl Iterator<Item = Result<DirEntry, walkdir::Error>>,
    filters: Vec<regex::Regex>,
    is_include_filter: bool,
    prefix_to_strip: PathBuf,
) -> impl Iterator<Item = FilterResult> {
    dir_iter.filter_map(|x| x.ok()).map(move |x| {
        // always make sure that we include directories
        if x.file_type().is_dir() {
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
        if let Some(_) = expr.find(&path.to_string_lossy()) {
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
