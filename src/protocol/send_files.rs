use super::Machine;
use crate::prelude::*;
use client::utils;
use tokio::io::AsyncWriteExt;

use super::built::{Built, ClientBuiltState, ServerBuiltState};

#[derive(Default)]
pub(crate) struct SendFiles;

pub(crate) struct ClientSendFilesState {
    pub(super) conn: transport::Connection<ClientMsg>,
    pub(super) working_dir: PathBuf,
    pub(super) job_name: String,
    pub(super) folder_state: client::BindingFolderState,
}

pub(crate) struct ServerSendFilesState {
    pub(super) conn: transport::Connection<ServerMsg>,
    pub(super) common: super::Common,
    pub(super) namespace: String,
    pub(super) batch_name: String,
    // the job identifier we have scheduled to run
    pub(super) job_identifier: server::JobIdentifier,
    pub(super) job_name: String,
    /// where the results of the job should be stored
    pub(super) save_location: PathBuf,
}

#[derive(thiserror::Error, Debug, From)]
pub(crate) enum ClientError {
    #[error("{0}")]
    TcpConnection(error::TcpConnection),
}

#[derive(thiserror::Error, Debug, From)]
pub(crate) enum ServerError {
    #[error("{0}")]
    TcpConnection(error::TcpConnection),
    #[error("{0}")]
    CreateDir(error::CreateDir),
}

impl From<(PathBuf, std::io::Error)> for ServerError {
    fn from(x: (PathBuf, std::io::Error)) -> Self {
        let err = error::CreateDir::new(x.1, x.0);
        Self::from(err)
    }
}

impl Machine<SendFiles, ClientSendFilesState> {
    /// read and send all files in the ./distribute_save folder that the job has left
    ///
    /// if there is an error reading a file, that file will be logged but not sent. The
    /// only possible error from this function is if the TCP connection is cut.
    pub(crate) async fn send_files(
        mut self,
    ) -> Result<Machine<Built, ClientBuiltState>, (Self, ClientError)> {
        let files = utils::read_save_folder(&self.state.working_dir);

        let dist_save_path = self.state.working_dir.join("distribute_save");

        for metadata in files.into_iter().skip(1) {
            // remove leading directories up until (and including) distribute_save

            debug!(
                "sending {} for job {}",
                metadata.file_path.display(),
                self.state.job_name,
            );

            match metadata.into_send_file() {
                Ok(mut send_file) => {
                    send_file.file_path =
                        utils::remove_path_prefixes(send_file.file_path, &dist_save_path);

                    let msg = ClientMsg::SaveFile(send_file);
                    let tmp = self.state.conn.transport_data(&msg).await;
                    throw_error_with_self!(tmp, self);

                    // receive the server telling us its ok to send the next file
                    let msg = self.state.conn.receive_data().await;
                    throw_error_with_self!(msg, self);
                }
                Err(e) => {
                    error!(
                        "failed to convert file metadata to file bytes when transporting: {} for job {}",
                        e, self.state.job_name
                    );
                }
            }
        }

        // tell the server we are done sending files and we should transition to the next state
        let msg = ClientMsg::FinishFiles;
        let tmp = self.state.conn.transport_data(&msg).await;
        throw_error_with_self!(tmp, self);

        let built_state = self.into_built_state().await;
        let machine = Machine::from_state(built_state);
        Ok(machine)
    }

    pub(crate) fn to_uninit(self) -> super::UninitClient {
        let ClientSendFilesState {
            conn, working_dir, ..
        } = self.state;
        let conn = conn.update_state();
        debug!("moving client send files -> uninit");
        let state = super::uninit::ClientUninitState { conn, working_dir };
        Machine::from_state(state)
    }

    async fn into_built_state(self) -> super::built::ClientBuiltState {
        debug!("moving client send files -> built");
        let ClientSendFilesState {
            conn,
            working_dir,
            folder_state,
            ..
        } = self.state;

        #[allow(unused_mut)]
        let mut conn = conn.update_state();

        #[cfg(test)]
        assert!(conn.bytes_left().await == 0);

        super::built::ClientBuiltState {
            conn,
            working_dir,
            folder_state,
        }
    }
}

impl Machine<SendFiles, ServerSendFilesState> {
    /// listen for the compute node to send us all the files that are in the ./distribute_save
    /// directory after the job has been completed
    pub(crate) async fn receive_files(
        mut self,
        scheduler_tx: &mut mpsc::Sender<server::JobRequest>,
    ) -> Result<Machine<Built, ServerBuiltState>, (Self, ServerError)> {
        // create the namesapce and batch name for this process
        let namespace_dir = self.state.common.save_path.join(&self.state.namespace);
        let batch_dir = namespace_dir.join(&self.state.batch_name);
        let job_dir = batch_dir.join(&self.state.job_name);

        // create directories, with error handling
        let create_dir = server::create_dir_helper::<ServerError>(&namespace_dir);
        throw_error_with_self!(create_dir, self);
        let create_dir = server::create_dir_helper::<ServerError>(&batch_dir);
        throw_error_with_self!(create_dir, self);
        let create_dir = server::create_dir_helper::<ServerError>(&job_dir);
        throw_error_with_self!(create_dir, self);

        loop {
            let msg = self.state.conn.receive_data().await;
            let msg: ClientMsg = throw_error_with_self!(msg, self);

            match msg {
                ClientMsg::SaveFile(file) => {
                    let path = self.state.save_location.join(file.file_path);

                    if file.is_file {
                        debug!(
                            "creating file {} on {} for {}",
                            path.display(),
                            self.state.common.node_name,
                            self.state.job_name
                        );

                        // save the file
                        let fs_file = tokio::fs::File::create(&path).await;
                        match fs_file {
                            Ok(mut f) => {
                                if let Err(e) = f.write_all(&file.bytes).await {
                                    error!("{}", e);
                                }
                            }
                            Err(e) => {
                                error!("{}", e);
                            }
                        }
                    } else {
                        debug!(
                            "creating directory {} on {} for {}",
                            path.display(),
                            self.state.common.node_name,
                            self.state.job_name
                        );

                        // create the directory for the file
                        let res = server::ok_if_exists(tokio::fs::create_dir(&path).await)
                            .map_err(|e| error::CreateDir::new(e, path));

                        if let Err(e) = res {
                            error!("failed to create the required directory to store the results of the job. This is very bad and should not happen: {}", e);
                        }
                    }
                }
                ClientMsg::FinishFiles => {
                    // first, tell the scheduler that this job has finished
                    if let Err(_e) = scheduler_tx
                        .send(server::pool_data::JobRequest::FinishJob(
                            self.state.job_identifier,
                        ))
                        .await
                    {
                        error!(
                            "scheduler is down - cannot transmit that job {} has finished on {}",
                            self.state.job_name, self.state.common.node_name
                        );
                        panic!(
                            "scheduler is down - cannot transmit that job {} has finished on {}",
                            self.state.job_name, self.state.common.node_name
                        );
                    }

                    // we are now done receiving files
                    let built_state = self.into_built_state().await;
                    let machine = Machine::from_state(built_state);
                    return Ok(machine);
                }
            }

            let tmp = self
                .state
                .conn
                .transport_data(&ServerMsg::ReceivedFile)
                .await;
            throw_error_with_self!(tmp, self);
        }
    }

     async fn into_built_state(self) -> super::built::ServerBuiltState {
        debug!("moving {} server send files -> built", self.state.common.node_name);
        let ServerSendFilesState {
            conn,
            common,
            namespace,
            batch_name,
            job_identifier,
            ..
        } = self.state;

        #[allow(unused_mut)]
        let mut conn = conn.update_state();

        #[cfg(test)]
        assert!(conn.bytes_left().await == 0);

        super::built::ServerBuiltState {
            conn,
            common,
            namespace,
            batch_name,
            job_identifier,
        }
    }

    pub(crate) fn to_uninit(self) -> super::UninitServer {
        let ServerSendFilesState { conn, common, .. } = self.state;
        let conn = conn.update_state();
        let state = super::uninit::ServerUninitState { conn, common };
        debug!("moving server send files -> uninit");
        Machine::from_state(state)
    }
}

#[derive(Serialize, Deserialize, Unwrap)]
pub(crate) enum ServerMsg {
    ReceivedFile,
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) enum ClientMsg {
    SaveFile(transport::SendFile),
    FinishFiles,
}

impl transport::AssociatedMessage for ServerMsg {
    type Receive = ClientMsg;
}

impl transport::AssociatedMessage for ClientMsg {
    type Receive = ServerMsg;
}
