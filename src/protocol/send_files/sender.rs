use super::Machine;
use crate::prelude::*;
use crate::server::pool_data::NodeMetadata;
use client::utils;
use tokio::io::AsyncWriteExt;

use super::super::built::{Built, ClientBuiltState};
use super::super::uninit::{ClientUninitState, Uninit};
use super::super::UninitClient;
use super::{ClientError, ClientMsg, NextState, SendFiles, ServerMsg, LARGE_FILE_BYTE_THRESHOLD};
use crate::client::execute::FileMetadata;

// in the job execution process, this is the client
pub(crate) struct SenderState<T> {
    pub(crate) conn: transport::Connection<ClientMsg>,
    pub(crate) extra: T,
}

pub(crate) trait FileSender {
    fn job_name(&self) -> &str;
    fn files_to_send(&self) -> Box<dyn Iterator<Item = FileMetadata> + Send>;
}

/// additional state used when performing file transfer
pub(crate) struct SenderFinalStore {
    pub(crate) job_name: String,
    pub(crate) folder_state: client::BindingFolderState,
    pub(crate) cancel_addr: SocketAddr,
    pub(crate) working_dir: PathBuf,
}

impl FileSender for SenderFinalStore {
    fn job_name(&self) -> &str {
        &self.job_name
    }
    fn files_to_send(&self) -> Box<dyn Iterator<Item = FileMetadata> + Send> {
        let folder_files_to_send = self.working_dir.join("distribute_save");
        let files = utils::read_folder_files(&folder_files_to_send);
        Box::new(files.into_iter().skip(1))
    }
}

impl NextState for SenderState<SenderFinalStore> {
    type Next = ClientBuiltState;
    type Marker = Built;

    fn next_state(self) -> Self::Next {
        info!("moving client send files -> built");
        let SenderState { conn, extra, .. } = self;

        let SenderFinalStore {
            working_dir,
            folder_state,
            cancel_addr,
            ..
        } = extra;

        #[allow(unused_mut)]
        let mut conn = conn.update_state();

        // TODO: find how to include this in non-async code
        //
        //#[cfg(test)]
        //assert!(conn.bytes_left().await == 0);

        ClientBuiltState {
            conn,
            working_dir,
            folder_state,
            cancel_addr,
        }
    }
}

/// additional state used when performing file transfer
pub(crate) struct FlatFileList {
    pub(crate) files: Vec<FileMetadata>,
}

impl NextState for SenderState<FlatFileList> {
    type Next = transport::Connection<ClientMsg>;
    type Marker = ();

    fn next_state(self) -> Self::Next {
        self.conn
    }
}

impl FileSender for FlatFileList {
    fn job_name(&self) -> &str {
        "user_file_send"
    }
    fn files_to_send(&self) -> Box<dyn Iterator<Item = FileMetadata> + Send> {
        Box::new(self.files.clone().into_iter())
    }
}

impl<T, NEXT, MARKER> Machine<SendFiles, SenderState<T>>
where
    T: FileSender,
    SenderState<T>: NextState<Marker = MARKER, Next = NEXT>,
    MARKER: Default,
{
    /// read and send all files in the ./distribute_save folder that the job has left
    ///
    /// if there is an error reading a file, that file will be logged but not sent. The
    /// only possible error from this function is if the TCP connection is cut.
    #[instrument(skip(self), fields(job_name=self.state.extra.job_name()))]
    pub(crate) async fn send_files(mut self) -> Result<Machine<MARKER, NEXT>, (Self, ClientError)> {
        let job_name = self.state.extra.job_name();
        let files_to_send = self.state.extra.files_to_send();

        for metadata in files_to_send {
            // remove leading directories up until (and including) distribute_save

            info!(
                "sending {} (relative: {})",
                metadata.absolute_file_path.display(),
                metadata.relative_file_path.display(),
            );

            let fs_meta =
                if let Ok(fs_meta) = tokio::fs::metadata(&metadata.absolute_file_path).await {
                    fs_meta
                } else {
                    error!(
                        "could not read metadata for {} - skipping",
                        metadata.absolute_file_path.display()
                    );
                    continue;
                };

            //
            // check if this file is very large
            //
            if fs_meta.len() > LARGE_FILE_BYTE_THRESHOLD && fs_meta.is_file() {
                let file_len = fs_meta.len();
                debug!("number of bytes being transported : {}", file_len);

                let path = metadata.absolute_file_path.clone();

                // strip out the useless prefixes to the path so it saves nicely on the other side
                let file =
                    ClientMsg::FileMarker(transport::FileMarker::new(metadata.relative_file_path));
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
                // .into_send_file() will take care of stripping the correct path prefixes for us
                match metadata.into_send_file() {
                    Ok(send_file) => {
                        let msg = ClientMsg::SaveFile(send_file);
                        let tmp = self.state.conn.transport_data(&msg).await;
                        throw_error_with_self!(tmp, self);

                        // receive the server telling us its ok to send the next file
                        let msg = self.state.conn.receive_data().await;
                        throw_error_with_self!(msg, self);
                    }
                    Err(e) => {
                        error!(
                            job_name = job_name,
                            err = %e,
                            "failed to convert file metadata to file bytes"
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

        let next_state = self.state.next_state();
        let machine = Machine::from_state(next_state);
        Ok(machine)
    }
}

impl Machine<SendFiles, SenderState<SenderFinalStore>> {
    pub(crate) fn into_uninit(self) -> UninitClient {
        let SenderState { conn, extra, .. } = self.state;

        let SenderFinalStore {
            working_dir,
            cancel_addr,
            ..
        } = extra;

        let conn = conn.update_state();
        info!("moving client send files -> uninit");
        let state = ClientUninitState {
            conn,
            working_dir,
            cancel_addr,
        };
        Machine::from_state(state)
    }
}
