mod receiver;
mod sender;

use super::Machine;
use crate::prelude::*;

pub(crate) use receiver::{
    BuildingReceiver, ExecutingReceiver, Nothing, ReceiverFinalStore, ReceiverState,
};
pub(crate) use sender::{
    BuildingSender, ExecutingSender, FlatFileList, SenderFinalStore, SenderState,
};

#[cfg(not(test))]
const LARGE_FILE_BYTE_THRESHOLD: u64 = 10u64.pow(9);
#[cfg(test)]
const LARGE_FILE_BYTE_THRESHOLD: u64 = 1000;

#[derive(Default, Debug)]
pub(crate) struct SendFiles;

pub(crate) trait NextState {
    type Next;
    type Marker;

    fn next_state(self) -> Self::Next;
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

    let base_dir = WorkingDir::from(PathBuf::from("./tests/send_files_with_large_files"));
    let save_path = base_dir.base().to_owned();

    // first, create the directories required and write some large files to them
    let dist_save = base_dir.distribute_save_folder();
    let file1 = dist_save.join(f1name);
    let file2 = dist_save.join(f2name);
    let file3 = dist_save.join(f3name);

    base_dir.delete_and_create_folders().await.unwrap();

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

    let extra = SenderFinalStore {
        working_dir: base_dir.clone(),
        job_name: "test job name".into(),
        folder_state: client::BindingFolderState::new(),
        cancel_addr,
    };

    let client_state = SenderState {
        conn: client_conn,
        extra,
    };

    let namespace = "namespace".to_string();
    let batch_name = "batch_name".to_string();
    let _job_name = "job_name".to_string();

    let run_info = server::pool_data::RunTaskInfo {
        namespace: namespace.clone(),
        batch_name: batch_name.clone(),
        identifier: server::JobSetIdentifier::none(),
        task: config::Job::placeholder_apptainer(),
    };

    let (_tx, common) = super::Common::test_configuration(
        add_port(client_port),
        add_port(client_keepalive),
        cancel_addr,
    );

    //
    // setup server state
    //
    let extra = ReceiverFinalStore { common, run_info };

    let server_state = ReceiverState {
        conn: server_conn,
        save_location: save_path.to_owned(),
        extra,
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

    std::fs::remove_dir_all(base_dir.base()).ok();
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
