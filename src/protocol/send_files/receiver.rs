use super::Machine;
use crate::prelude::*;
use crate::server::pool_data::NodeMetadata;

use tokio::io::AsyncWriteExt;

use super::super::built::{self};
use super::super::uninit::{self, ClientUninitState};
use super::super::{UninitClient, UninitServer};
use super::NextState;
use super::{ClientMsg, SendFiles, ServerError, ServerMsg};

// in the job execution process, this is the server
pub(crate) struct ReceiverState<T> {
    pub(crate) conn: transport::Connection<ServerMsg>,
    /// where the results of the job should be stored
    pub(crate) save_location: PathBuf,
    pub(crate) extra: T,
}

/// additional state used when performing file transfer
pub(crate) struct ReceiverFinalStore {
    pub(crate) run_info: server::pool_data::RunTaskInfo,
    pub(crate) common: super::super::Common,
}

pub(crate) trait SendLogging {
    fn job_identifier(&self) -> JobSetIdentifier;
    fn namespace(&self) -> &str;
    fn batch_name(&self) -> &str;
    fn job_name(&self) -> &str;
    fn node_meta(&self) -> &NodeMetadata;
}

impl SendLogging for ReceiverFinalStore {
    fn job_identifier(&self) -> JobSetIdentifier {
        self.run_info.identifier
    }

    fn namespace(&self) -> &str {
        &self.run_info.namespace
    }

    fn batch_name(&self) -> &str {
        &self.run_info.batch_name
    }

    fn job_name(&self) -> &str {
        &self.run_info.task.name()
    }

    fn node_meta(&self) -> &NodeMetadata {
        &self.common.node_meta
    }
}

impl NextState for ReceiverState<ReceiverFinalStore> {
    type Next = built::ServerBuiltState;
    type Marker = built::Built;

    fn next_state(self) -> Self::Next {
        debug!(
            "moving {} server send files -> built",
            self.extra.node_meta()
        );

        let ReceiverState { conn, extra, .. } = self;

        let ReceiverFinalStore {
            common, run_info, ..
        } = extra;

        let namespace = run_info.namespace;
        let batch_name = run_info.batch_name;
        let job_identifier = run_info.identifier;

        #[allow(unused_mut)]
        let mut conn = conn.update_state();

        //#[cfg(test)]
        //{
        //    info!("checking for remaining bytes on server side...");
        //    assert!(conn.bytes_left().await == 0);
        //}

        built::ServerBuiltState {
            conn,
            common,
            namespace,
            batch_name,
            job_identifier,
        }
    }
}

pub(crate) struct Nothing {
    node_meta: NodeMetadata,
}

impl Nothing {
    pub(crate) fn new() -> Self {
        let node_meta = NodeMetadata::new("USER".into(), ([0, 0, 0, 0], 0).into());
        Self { node_meta }
    }
}

impl NextState for ReceiverState<Nothing> {
    type Next = transport::Connection<ServerMsg>;
    type Marker = ();

    fn next_state(self) -> Self::Next {
        self.conn
    }
}

impl SendLogging for Nothing {
    fn job_identifier(&self) -> JobSetIdentifier {
        JobSetIdentifier::none()
    }

    fn namespace(&self) -> &str {
        "USER"
    }

    fn batch_name(&self) -> &str {
        "USER"
    }

    fn job_name(&self) -> &str {
        "USER"
    }

    fn node_meta(&self) -> &NodeMetadata {
        &self.node_meta
    }
}

pub(crate) struct BuildingReceiver {
    pub(crate) node_meta: NodeMetadata,
    pub(crate) working_dir: WorkingDir,
    pub(crate) cancel_addr: SocketAddr,
    pub(crate) build_info: server::pool_data::BuildTaskInfo,
}

impl NextState for ReceiverState<BuildingReceiver> {
    type Next = protocol::compiling::ClientBuildingState;
    type Marker = protocol::compiling::Building;

    fn next_state(self) -> Self::Next {
        let ReceiverState {
            conn,
            save_location: _,
            extra,
        } = self;
        let BuildingReceiver {
            working_dir,
            cancel_addr,
            build_info,
            node_meta: _,
        } = extra;

        let conn = conn.update_state();

        protocol::compiling::ClientBuildingState {
            build_info,
            conn,
            working_dir,
            cancel_addr,
        }
    }
}

impl SendLogging for BuildingReceiver {
    fn job_identifier(&self) -> JobSetIdentifier {
        JobSetIdentifier::none()
    }

    fn namespace(&self) -> &str {
        "SERVER"
    }

    fn batch_name(&self) -> &str {
        "USER"
    }

    fn job_name(&self) -> &str {
        "UNKNOWN"
    }

    fn node_meta(&self) -> &NodeMetadata {
        &self.node_meta
    }
}

pub(crate) struct ExecutingReceiver {
    pub(crate) node_meta: NodeMetadata,
    pub(crate) working_dir: WorkingDir,
    pub(crate) cancel_addr: SocketAddr,
    pub(crate) run_info: server::pool_data::RunTaskInfo,
    pub(crate) folder_state: client::BindingFolderState,
}

impl NextState for ReceiverState<ExecutingReceiver> {
    type Next = protocol::executing::ClientExecutingState;
    type Marker = protocol::executing::Executing;

    fn next_state(self) -> Self::Next {
        let ReceiverState {
            conn,
            save_location: _,
            extra,
        } = self;
        let ExecutingReceiver {
            working_dir,
            cancel_addr,
            run_info,
            node_meta: _,
            folder_state,
        } = extra;

        let conn = conn.update_state();

        protocol::executing::ClientExecutingState {
            run_info,
            conn,
            working_dir,
            cancel_addr,
            folder_state,
        }
    }
}

impl SendLogging for ExecutingReceiver {
    fn job_identifier(&self) -> JobSetIdentifier {
        JobSetIdentifier::none()
    }

    fn namespace(&self) -> &str {
        "SERVER"
    }

    fn batch_name(&self) -> &str {
        "USER"
    }

    fn job_name(&self) -> &str {
        "UNKNOWN"
    }

    fn node_meta(&self) -> &NodeMetadata {
        &self.node_meta
    }
}

impl<T, NEXT, MARKER> Machine<SendFiles, ReceiverState<T>>
where
    T: SendLogging,
    ReceiverState<T>: NextState<Next = NEXT, Marker = MARKER>,
    MARKER: Default,
{
    /// listen for the compute node to send us all the files that are in the ./distribute_save
    /// directory after the job has been completed
    #[instrument(
        skip(self, scheduler_tx), 
        fields(
            node_meta = %self.state.extra.node_meta(),
            namespace = self.state.extra.namespace(),
            batch_name = self.state.extra.batch_name(),
            job_name = self.state.extra.job_name(),
        )
    )]
    pub(crate) async fn receive_files(
        mut self,
        scheduler_tx: &mut mpsc::Sender<server::JobRequest>,
    ) -> Result<Machine<MARKER, NEXT>, (Self, ServerError)> {
        let save_location = &self.state.save_location;

        // create directories recursively, with error handling
        let create_dir = server::create_dir_all_helper::<ServerError>(&save_location);
        throw_error_with_self!(create_dir, self);

        loop {
            let msg = self.state.conn.receive_data().await;
            let msg: ClientMsg = throw_error_with_self!(msg, self);

            match msg {
                ClientMsg::SaveFile(file) => {
                    let path = self.state.save_location.join(&file.file_path);

                    if file.is_file {
                        debug!(
                            "creating file {} (orig. path {}",
                            path.display(),
                            file.file_path.display()
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
                            self.state.extra.node_meta(),
                            self.state.extra.job_name()
                        );

                        // create the directory for the file
                        let res = server::ok_if_exists(tokio::fs::create_dir(&path).await)
                            .map_err(|e| error::CreateDir::new(e, path));

                        if let Err(e) = res {
                            error!("failed to create the required directory to store the results of the job. This is very bad and should not happen: {}", e);
                        }
                    }
                }
                // signals that the next file sent will be very large, and we should not attempt to
                // read it into memory
                ClientMsg::FileMarker(marker) => {
                    let path = self.state.save_location.join(marker.file_path);
                    let fs_file = tokio::fs::File::create(&path).await;

                    let file = match fs_file {
                        Ok(f) => {
                            debug!(
                                "created path for large file at {}, telling client to proceed",
                                path.display()
                            );
                            let msg = ServerMsg::AwaitingLargeFile;
                            throw_error_with_self!(
                                self.state.conn.transport_data(&msg).await,
                                self
                            );
                            f
                        }
                        Err(e) => {
                            error!("failed to create file for path {} ({}), telling the client not to send the file", path.display(), e);
                            let msg = ServerMsg::DontSendLargeFile;
                            throw_error_with_self!(
                                self.state.conn.transport_data(&msg).await,
                                self
                            );
                            continue;
                        }
                    };

                    // now the sender should send the large file directly from disk
                    let mut writer = file;

                    throw_error_with_self!(
                        self.state.conn.receive_to_writer(&mut writer).await,
                        self
                    );

                    // flush the contents of the buffer to the file
                    writer.flush().await.unwrap();
                }
                ClientMsg::FinishFiles => {
                    // first, tell the scheduler that this job has finished
                    let finish_msg = server::pool_data::FinishJob {
                        ident: self.state.extra.job_identifier(),
                        job_name: self.state.extra.job_name().to_string(),
                    };
                    if let Err(_e) = scheduler_tx
                        .send(server::pool_data::JobRequest::FinishJob(finish_msg))
                        .await
                    {
                        error!(
                            "scheduler is down - cannot transmit that job {} has finished on {}",
                            self.state.extra.job_name(),
                            self.state.extra.node_meta()
                        );
                        panic!(
                            "scheduler is down - cannot transmit that job {} has finished on {}",
                            self.state.extra.job_name(),
                            self.state.extra.node_meta()
                        );
                    }

                    // we are now done receiving files
                    let built_state = self.state.next_state();
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
}

impl Machine<SendFiles, ReceiverState<ReceiverFinalStore>> {
    pub(crate) fn into_uninit(self) -> (UninitServer, server::pool_data::RunTaskInfo) {
        let ReceiverState { conn, extra, .. } = self.state;

        let ReceiverFinalStore {
            common, run_info, ..
        } = extra;

        let conn = conn.update_state();
        let state = uninit::ServerUninitState { conn, common };

        debug!(
            "moving {} server send files -> uninit",
            state.common.node_meta
        );

        (Machine::from_state(state), run_info)
    }

    pub(crate) fn node_name(&self) -> &str {
        &self.state.extra.common.node_meta.node_name
    }

    pub(crate) fn job_name(&self) -> &str {
        &self.state.extra.run_info.task.name()
    }
}

impl Machine<SendFiles, ReceiverState<BuildingReceiver>> {
    pub(crate) fn into_uninit(self) -> UninitClient {
        let ReceiverState { conn, extra, .. } = self.state;

        let BuildingReceiver {
            working_dir,
            cancel_addr,
            ..
        } = extra;

        let conn = conn.update_state();
        info!("moving client send files(building) -> uninit");
        let state = ClientUninitState {
            conn,
            working_dir,
            cancel_addr,
        };

        Machine::from_state(state)
    }
}

impl Machine<SendFiles, ReceiverState<ExecutingReceiver>> {
    pub(crate) fn into_uninit(self) -> UninitClient {
        let ReceiverState { conn, extra, .. } = self.state;

        let ExecutingReceiver {
            working_dir,
            cancel_addr,
            ..
        } = extra;

        let conn = conn.update_state();
        info!("moving client send files(executing) -> uninit");
        let state = ClientUninitState {
            conn,
            working_dir,
            cancel_addr,
        };

        Machine::from_state(state)
    }
}
