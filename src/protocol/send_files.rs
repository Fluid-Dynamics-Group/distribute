use super::Machine;
use crate::prelude::*;
use client::utils;

use super::built::{Built, ClientBuiltState, ServerBuiltState};

pub(crate) struct SendFiles;
pub(crate) struct ClientSendFilesState {
    conn: transport::Connection<ClientMsg>,
    working_dir: PathBuf,
    job_name: String,
    folder_state: client::BindingFolderState,
}

pub(crate) struct ServerSendFilesState {
    conn: transport::Connection<ServerMsg>,
    common: super::Common,
    namespace: String,
    batch_name: String,
    // the job identifier we have scheduled to run
    job_identifier: server::JobIdentifier,
    job_name: String,
    /// where the results of the job should be stored
    save_location: PathBuf,
}

#[derive(thiserror::Error, Debug, From)]
pub(crate) enum ClientError {
    #[error("{0}")]
    TcpConnection(error::TcpConnection),
    //#[error("{0}")]
    //CreateDir(error::CreateDir),
}

#[derive(thiserror::Error, Debug, From)]
pub(crate) enum ServerError {
    #[error("{0}")]
    TcpConnection(error::TcpConnection),
    #[error("{0}")]
    CreateDir(error::CreateDir),
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
                        "failed to convert file metadata to file bytes when transporting: {}",
                        e
                    );
                }
            }
        }

        // tell the server we are done sending files and we should transition to the next state
        let msg = ClientMsg::FinishFiles;
        let tmp = self.state.conn.transport_data(&msg).await;
        throw_error_with_self!(tmp, self);

        todo!()
    }

    pub(crate) fn to_uninit(self) -> super::UninitClient {
        todo!()
    }
}

impl Machine<SendFiles, ServerSendFilesState> {
    /// listen for the compute node to send us all the files that are in the ./distribute_save
    /// directory after the job has been completed
    pub(crate) async fn send_files(
        mut self,
    ) -> Result<Machine<Built, ServerBuiltState>, (Self, ServerError)> {
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
                        let res = tokio::fs::write(&path, file.bytes)
                            .await
                            .map_err(|e| error::WriteFile::new(e, path));

                        // if there was an error writing the file then log it
                        if let Err(e) = res {
                            error!("{}", e)
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
                        throw_error_with_self!(res, self);
                    }
                }
                ClientMsg::FinishFiles => {
                    // we are now done receiving files

                    // TODO: move to Machine<Built, _> state
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

    pub(crate) fn to_uninit(self) -> super::UninitClient {
        todo!()
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