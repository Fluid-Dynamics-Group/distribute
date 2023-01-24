use super::Machine;
use crate::prelude::*;
use client::utils;
use tokio::io::AsyncWriteExt;
use crate::server::pool_data::NodeMetadata;

use super::super::built::{Built, ClientBuiltState};
use super::super::uninit::{Uninit, ClientUninitState};
use super::super::UninitClient;
use super::{SendFiles, ClientMsg, ServerMsg, ClientError, LARGE_FILE_BYTE_THRESHOLD};

// in the job execution process, this is the client
pub(crate) struct SenderState {
    pub(super) conn: transport::Connection<ClientMsg>,
    pub(super) working_dir: PathBuf,
    pub(super) job_name: String,
    pub(super) folder_state: client::BindingFolderState,
    pub(super) cancel_addr: SocketAddr,
}

impl Machine<SendFiles, SenderState> {
    /// read and send all files in the ./distribute_save folder that the job has left
    ///
    /// if there is an error reading a file, that file will be logged but not sent. The
    /// only possible error from this function is if the TCP connection is cut.
    #[instrument(skip(self), fields(job_name=self.state.job_name))]
    pub(crate) async fn send_files(
        mut self,
    ) -> Result<Machine<Built, ClientBuiltState>, (Self, ClientError)> {
        let files = utils::read_save_folder(&self.state.working_dir);

        let dist_save_path = self.state.working_dir.join("distribute_save");

        for metadata in files.into_iter().skip(1) {
            // remove leading directories up until (and including) distribute_save

            info!(
                "sending {} for job {}",
                metadata.file_path.display(),
                self.state.job_name,
            );

            let fs_meta = if let Ok(fs_meta) = tokio::fs::metadata(&metadata.file_path).await {
                fs_meta
            } else {
                error!(
                    "could not read metadata for {} - skipping",
                    metadata.file_path.display()
                );
                continue;
            };

            //
            // check if this file is very large
            //
            if fs_meta.len() > LARGE_FILE_BYTE_THRESHOLD && fs_meta.is_file() {
                let file_len = fs_meta.len();
                debug!("number of bytes being transported : {}", file_len);

                let path = metadata.file_path.clone();

                // strip out the useless prefixes to the path so it saves nicely on the other side
                let abbreviated_path =
                    utils::remove_path_prefixes(metadata.file_path, &dist_save_path);
                let file = ClientMsg::FileMarker(transport::FileMarker::new(abbreviated_path));
                throw_error_with_self!(self.state.conn.transport_data(&file).await, self);

                let msg = throw_error_with_self!(self.state.conn.receive_data().await, self);

                match msg {
                    ServerMsg::AwaitingLargeFile => {
                        let mut reader = if let Ok(rdr) = tokio::fs::File::open(&path).await {
                            rdr
                        } else {
                            error!("could not unwrap large file after we checked its metadata, this should not happen - panicking");
                            panic!("could not unwrap large file after we checked its metadata, this should not happen");
                        };

                        self.state
                            .conn
                            .transport_from_reader(&mut reader, file_len)
                            .await
                            .ok();

                        // receive the server telling us its ok to send the next file
                        let msg = self.state.conn.receive_data().await;
                        throw_error_with_self!(msg, self);
                    }
                    ServerMsg::ReceivedFile | ServerMsg::DontSendLargeFile => {
                        info!("skipping large file transport to server {}", path.display());
                        continue;
                    }
                }
            }
            //
            // file is small - send the file in memory
            //
            else {
                match metadata.into_send_file() {
                    Ok(mut send_file) => {
                        // strip the path prefix so the other side does not botch the path to save
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
        }

        // tell the server we are done sending files and we should transition to the next state
        info!("done sending files - transitioning to next state");

        let msg = ClientMsg::FinishFiles;
        let tmp = self.state.conn.transport_data(&msg).await;
        throw_error_with_self!(tmp, self);

        let built_state = self.into_built_state().await;
        let machine = Machine::from_state(built_state);
        Ok(machine)
    }

    pub(crate) fn into_uninit(self) -> UninitClient {
        let SenderState {
            conn,
            working_dir,
            cancel_addr,
            ..
        } = self.state;
        let conn = conn.update_state();
        info!("moving client send files -> uninit");
        let state = ClientUninitState {
            conn,
            working_dir,
            cancel_addr,
        };
        Machine::from_state(state)
    }

    async fn into_built_state(self) -> ClientBuiltState {
        info!("moving client send files -> built");
        let SenderState {
            conn,
            working_dir,
            folder_state,
            cancel_addr,
            ..
        } = self.state;

        #[allow(unused_mut)]
        let mut conn = conn.update_state();

        #[cfg(test)]
        assert!(conn.bytes_left().await == 0);

        ClientBuiltState {
            conn,
            working_dir,
            folder_state,
            cancel_addr,
        }
    }
}
