use super::Machine;
use crate::prelude::*;
use client::utils;
use tokio::io::AsyncWriteExt;

use super::built::{Built, ClientBuiltState, ServerBuiltState};

#[cfg(not(test))]
const LARGE_FILE_BYTE_THRESHOLD: u64 = 10u64.pow(9);
#[cfg(test)]
const LARGE_FILE_BYTE_THRESHOLD: u64 = 1000;

#[derive(Default)]
pub(crate) struct SendFiles;

pub(crate) struct ClientSendFilesState {
    pub(super) conn: transport::Connection<ClientMsg>,
    pub(super) working_dir: PathBuf,
    pub(super) job_name: String,
    pub(super) folder_state: client::BindingFolderState,
    pub(super) cancel_addr: SocketAddr,
}

pub(crate) struct ServerSendFilesState {
    pub(super) conn: transport::Connection<ServerMsg>,
    pub(super) common: super::Common,
    pub(super) job_name: String,
    /// where the results of the job should be stored
    pub(super) save_location: PathBuf,
    pub(super) task_info: server::pool_data::RunTaskInfo,
}

impl ServerSendFilesState {
    fn job_identifier(&self) -> JobIdentifier {
        self.task_info.identifier
    }

    fn namespace(&self) -> &str {
        &self.task_info.namespace
    }

    fn batch_name(&self) -> &str {
        &self.task_info.batch_name
    }
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
                        debug!("skipping large file transport to server {}", path.display());
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
        debug!("done sending files - transitioning to next state");

        let msg = ClientMsg::FinishFiles;
        let tmp = self.state.conn.transport_data(&msg).await;
        throw_error_with_self!(tmp, self);

        let built_state = self.into_built_state().await;
        let machine = Machine::from_state(built_state);
        Ok(machine)
    }

    pub(crate) fn into_uninit(self) -> super::UninitClient {
        let ClientSendFilesState {
            conn,
            working_dir,
            cancel_addr,
            ..
        } = self.state;
        let conn = conn.update_state();
        debug!("moving client send files -> uninit");
        let state = super::uninit::ClientUninitState {
            conn,
            working_dir,
            cancel_addr,
        };
        Machine::from_state(state)
    }

    async fn into_built_state(self) -> super::built::ClientBuiltState {
        debug!("moving client send files -> built");
        let ClientSendFilesState {
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

        super::built::ClientBuiltState {
            conn,
            working_dir,
            folder_state,
            cancel_addr,
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
        let namespace_dir = self.state.common.save_path.join(&self.state.namespace());
        let batch_dir = namespace_dir.join(&self.state.batch_name());
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
                    if let Err(_e) = scheduler_tx
                        .send(server::pool_data::JobRequest::FinishJob(
                            self.state.job_identifier(),
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
        debug!(
            "moving {} server send files -> built",
            self.state.common.node_name
        );
        let ServerSendFilesState {
            conn,
            common,
            task_info,
            ..
        } = self.state;

        let namespace = task_info.namespace;
        let batch_name = task_info.batch_name;
        let job_identifier = task_info.identifier;

        #[allow(unused_mut)]
        let mut conn = conn.update_state();

        #[cfg(test)]
        {
            info!("checking for remaining bytes on server side...");
            assert!(conn.bytes_left().await == 0);
        }

        super::built::ServerBuiltState {
            conn,
            common,
            namespace,
            batch_name,
            job_identifier,
        }
    }

    pub(crate) fn into_uninit(self) -> (super::UninitServer, server::pool_data::RunTaskInfo) {
        let ServerSendFilesState {
            conn,
            common,
            task_info,
            ..
        } = self.state;
        let conn = conn.update_state();
        let state = super::uninit::ServerUninitState { conn, common };
        debug!("moving server send files -> uninit");

        (Machine::from_state(state), task_info)
    }

    pub(crate) fn node_name(&self) -> &str {
        &self.state.common.node_name
    }

    pub(crate) fn job_name(&self) -> &str {
        &self.state.job_name
    }
}

#[derive(Serialize, Deserialize, Unwrap)]
#[allow(clippy::enum_variant_names)]
pub(crate) enum ServerMsg {
    ReceivedFile,
    AwaitingLargeFile,
    DontSendLargeFile,
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) enum ClientMsg {
    SaveFile(transport::SendFile),
    FileMarker(transport::FileMarker),
    FinishFiles,
}

impl transport::AssociatedMessage for ServerMsg {
    type Receive = ClientMsg;
}

impl transport::AssociatedMessage for ClientMsg {
    type Receive = ServerMsg;
}

#[tokio::test]
#[serial_test::serial]
async fn transport_files_with_large_file() {
    if false {
        crate::logger()
    }

    let add_port = |port| return SocketAddr::from(([0, 0, 0, 0], port));

    let f1name = "small_file.txt";
    let f2name = "large_file.binary";
    let f3name = "another_small_file.txt";

    let base_dir = PathBuf::from("./tests/send_files_with_large_files");
    let save_path = base_dir.clone();
    // first, create the directories required and write some large files to them
    let dist_save = base_dir.join("distribute_save");
    let file1 = dist_save.join(f1name);
    let file2 = dist_save.join(f2name);
    let file3 = dist_save.join(f3name);

    std::fs::remove_dir_all(&base_dir).ok();
    std::fs::create_dir(&base_dir).unwrap();
    std::fs::create_dir(&dist_save).unwrap();

    {
        std::fs::File::create(file1)
            .unwrap()
            .write_all(&vec![0; 100])
            .unwrap();
        std::fs::File::create(file2)
            .unwrap()
            .write_all(&vec![0; 2000])
            .unwrap();
        std::fs::File::create(file3)
            .unwrap()
            .write_all(&vec![0; 20])
            .unwrap();
    }

    let client_port = 10_000;
    let client_keepalive = 10_001;
    let cancel_port = 10_004;

    let cancel_addr = add_port(cancel_port);

    let client_listener = tokio::net::TcpListener::bind(add_port(client_port))
        .await
        .unwrap();
    let server_conn = tokio::net::TcpStream::connect(add_port(client_port))
        .await
        .unwrap();
    let client_conn = client_listener.accept().await.unwrap().0;

    let server_conn = transport::Connection::from_connection(server_conn);
    let client_conn = transport::Connection::from_connection(client_conn);

    let client_state = ClientSendFilesState {
        conn: client_conn,
        working_dir: base_dir.clone(),
        job_name: "test job name".into(),
        folder_state: client::BindingFolderState::new(),
        cancel_addr,
    };

    let namespace = "namespace".to_string();
    let batch_name = "batch_name".to_string();
    let job_name = "job_name".to_string();

    let task_info = server::pool_data::RunTaskInfo {
        namespace: namespace.clone(),
        batch_name: batch_name.clone(),
        identifier: server::JobIdentifier::none(),
        task: transport::JobOpt::placeholder_test_data(),
    };

    let (_tx, common) = super::Common::test_configuration(
        add_port(client_port),
        add_port(client_keepalive),
        cancel_addr,
    );
    let server_state = ServerSendFilesState {
        conn: server_conn,
        common,
        task_info,
        job_name,
        save_location: save_path.clone(),
    };

    let server_machine = Machine::from_state(server_state);
    let client_machine = Machine::from_state(client_state);

    let (tx_server, rx_server) = oneshot::channel();
    let (tx_client, rx_client) = oneshot::channel();

    let (tx_sched, _rx_sched) = mpsc::channel(10);

    let server_handle = tokio::spawn(async move {
        let mut tx_sched = tx_sched;
        match server_machine.receive_files(&mut tx_sched).await {
            Ok(_) => {
                eprintln!("server finished");
                tx_server.send(true).unwrap();
            }
            Err((_, e)) => {
                eprintln!("error with server: {}", e);
                tx_server.send(false).unwrap();
            }
        }
    });

    let client_handle = tokio::spawn(async move {
        match client_machine.send_files().await {
            Ok(_) => {
                eprintln!("client finished");
                tx_client.send(true).unwrap();
            }
            Err((_, e)) => {
                eprintln!("error with client: {}", e);
                tx_client.send(false).unwrap();
            }
        }
    });

    // execute the tasks
    tokio::time::timeout(
        Duration::from_secs(2),
        futures::future::join_all([server_handle, client_handle]),
    )
    .await
    .expect("futures did not complete in time");

    // check that the client returns something
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(5), rx_client)
            .await
            .unwrap()
            .unwrap(),
        true
    );

    // check that the server returns something
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(5), rx_server)
            .await
            .unwrap()
            .unwrap(),
        true
    );

    assert_eq!(save_path.join(f1name).exists(), true);
    assert_eq!(
        save_path.join(f2name).exists(),
        true,
        "large file does not exist"
    );
    assert_eq!(save_path.join(f3name).exists(), true);

    assert_eq!(
        std::fs::metadata(save_path.join(f1name)).unwrap().len(),
        100
    );
    assert_eq!(std::fs::metadata(save_path.join(f3name)).unwrap().len(), 20);
    assert_eq!(
        std::fs::metadata(save_path.join(f2name)).unwrap().len(),
        2000
    );

    //std::fs::remove_dir_all(base_dir).ok();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn async_read_from_file() {
    let path = PathBuf::from("./tests/test_file.binary");
    std::fs::File::create(&path)
        .unwrap()
        .write_all(&vec![0; 1000])
        .unwrap();

    let mut file = tokio::fs::File::open(&path).await.unwrap();

    let mut buffer = [0; 100];
    let num_bytes = file.read(&mut buffer).await.unwrap();

    let len = tokio::fs::metadata(&path).await.unwrap().len();

    std::fs::remove_file(&path).ok();

    dbg!(num_bytes);
    assert!(num_bytes > 0);

    assert_eq!(len, 1000);
}
