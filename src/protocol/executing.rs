use super::Either;
use super::Machine;
use crate::prelude::*;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;

use super::send_files::{ClientSendFilesState, SendFiles, ServerSendFilesState};

#[derive(Default)]
pub(crate) struct Executing;

pub(crate) struct ClientExecutingState {
    pub(super) conn: transport::Connection<ClientMsg>,
    pub(super) working_dir: PathBuf,
    pub(super) job: transport::JobOpt,
    pub(super) folder_state: client::BindingFolderState,
    pub(super) cancel_addr: SocketAddr,
}

pub(crate) struct ServerExecutingState {
    pub(super) conn: transport::Connection<ServerMsg>,
    pub(super) common: super::Common,
    pub(super) namespace: String,
    pub(super) batch_name: String,
    // the job identifier we have scheduled to run
    pub(super) job_identifier: server::JobIdentifier,
    pub(super) job_name: String,
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
}

impl Machine<Executing, ClientExecutingState> {
    pub(crate) async fn execute_job(
        mut self,
    ) -> Result<
        super::ClientEitherPrepareBuild<Machine<SendFiles, ClientSendFilesState>>,
        (Self, ClientError),
    > {
        // TODO: this broadcast can be made a oneshot
        let (tx_cancel, mut rx_cancel) = broadcast::channel(1);
        let is_cancelled = Arc::new(AtomicBool::new(false));

        let working_dir = self.state.working_dir.clone();
        let job = self.state.job.clone();

        // start an arbiter to monitor the cancellation port and receive information
        // from the main compute process about our job.
        // This will spawn a background task to constantly check the port so that we dont
        // block the execution of the current process.
        let arbiter =
            CancelArbiter::new(self.state.cancel_addr, tx_cancel, Arc::clone(&is_cancelled));

        // execute the job on the current task. Each client::run_(X)_job provices a
        // tokio::select call that will monitor the broadcast receiver (whose transmitter
        // belongs on the arbiter task) for a cancellation from the port.
        //
        // This is required because we cannot simultaneously run the task and cancel the
        // task with the same `tokio::select!` call
        let msg = match job {
            transport::JobOpt::Python(python_job) => {
                let run_result =
                    client::run_python_job(python_job, &working_dir, &mut rx_cancel).await;
                ClientMsg::from_run_result(run_result)
            }
            transport::JobOpt::Apptainer(apptainer_job) => {
                let run_result = client::run_apptainer_job(
                    apptainer_job,
                    &working_dir,
                    &mut rx_cancel,
                    &self.state.folder_state,
                )
                .await;
                ClientMsg::from_run_result(run_result)
            }
        };

        // stop monitoring the cancellation port, the job is now done
        arbiter.stop().await;

        let msg = if is_cancelled.load(Ordering::Relaxed) {
            ClientMsg::Cancelled
        } else {
            msg
        };

        let tmp = self.state.conn.transport_data(&msg).await;
        throw_error_with_self!(tmp, self);

        //
        // after sending the head node what our state is, lets just one last time check
        // that we are indeed working towards the correct state.
        //
        let tmp_state_confirm = self.state.conn.receive_data().await;
        let state_confirm = throw_error_with_self!(tmp_state_confirm, self);

        // if we are overriding to a cancellation state, then adjust the message,
        // otherwise continue as normal
        let msg = if matches!(state_confirm, ServerMsg::OverrideWithCancellation) {
            ClientMsg::Cancelled
        } else {
            msg
        };

        match msg {
            ClientMsg::Cancelled => {
                // go to Machine<PrepareBuild, _>
                let prepare_build = self.into_prepare_build_state().await;
                let machine = Machine::from_state(prepare_build);
                let either = Either::Left(machine);
                Ok(either)
            }
            ClientMsg::Successful | ClientMsg::Failed => {
                // go to Machine<SendFiles, _>
                let send_files = self.into_send_files_state().await;
                let machine = Machine::from_state(send_files);
                let either = Either::Right(machine);
                Ok(either)
            }
        }
    }

    pub(crate) fn into_uninit(self) -> super::UninitClient {
        let ClientExecutingState {
            conn,
            working_dir,
            cancel_addr,
            ..
        } = self.state;
        let conn = conn.update_state();
        let state = super::uninit::ClientUninitState {
            conn,
            working_dir,
            cancel_addr,
        };
        debug!("moving client executing -> uninit");
        Machine::from_state(state)
    }

    async fn into_send_files_state(self) -> super::send_files::ClientSendFilesState {
        debug!("moving client executing -> send files");
        let ClientExecutingState {
            conn,
            working_dir,
            job,
            folder_state,
            cancel_addr,
        } = self.state;

        #[allow(unused_mut)]
        let mut conn = conn.update_state();

        #[cfg(test)]
        assert!(conn.bytes_left().await == 0);

        let job_name = job.name().to_string();
        super::send_files::ClientSendFilesState {
            conn,
            working_dir,
            job_name,
            folder_state,
            cancel_addr,
        }
    }

    async fn into_prepare_build_state(self) -> super::prepare_build::ClientPrepareBuildState {
        let ClientExecutingState {
            conn,
            working_dir,
            cancel_addr,
            ..
        } = self.state;
        debug!("moving client executing -> prepare build");

        #[allow(unused_mut)]
        let mut conn = conn.update_state();

        #[cfg(test)]
        assert!(conn.bytes_left().await == 0);
        super::prepare_build::ClientPrepareBuildState {
            conn,
            working_dir,
            cancel_addr,
        }
    }
}

impl Machine<Executing, ServerExecutingState> {
    pub(crate) async fn wait_job_execution(
        mut self,
    ) -> Result<
        super::ServerEitherPrepareBuild<Machine<SendFiles, ServerSendFilesState>>,
        (Self, ServerError),
    > {
        let mut cancelled = false;

        let cancel_checker = server::node::check_broadcast_for_matching_token(
            &mut self.state.common.receive_cancellation,
            &self.state.common.cancel_addr,
            self.state.job_identifier,
            &mut cancelled,
        );

        let msg_result = tokio::select!(
            msg = self.state.conn.receive_data() => {
                msg
            }
            _ = cancel_checker => {
                // cancel_checker will never return
                unreachable!()
            }
        );

        let msg = throw_error_with_self!(msg_result, self);

        match msg {
            ClientMsg::Cancelled => {
                // tell the compute node to continue as scheduled
                info!("instructing compute node to transition to PrepareBuild state as planned");
                let tmp = self.state.conn.transport_data(&ServerMsg::Continue).await;
                throw_error_with_self!(tmp, self);

                // go to Machine<PrepareBuild, _>
                let prepare_build = self.into_prepare_build_state().await;
                let machine = Machine::from_state(prepare_build);
                let either = Either::Left(machine);

                Ok(either)
            }
            ClientMsg::Successful | ClientMsg::Failed => {
                // first, make sure we are both syncing our state
                // and transitioning to the correct state machines
                if cancelled {
                    info!("telling the compute node to NOT move to {msg:?}, instead to move to cancelled state");
                    let tmp = self
                        .state
                        .conn
                        .transport_data(&ServerMsg::OverrideWithCancellation)
                        .await;
                    throw_error_with_self!(tmp, self);
                } else {
                    info!("instructing compute node to transition to SendFiles state as planned");
                    let tmp = self.state.conn.transport_data(&ServerMsg::Continue).await;
                    throw_error_with_self!(tmp, self);
                }

                // go to Machine<SendFiles, _>
                let send_files = self.into_send_files_state().await;
                let machine = Machine::from_state(send_files);
                let either = Either::Right(machine);
                Ok(either)
            }
        }
    }

    pub(crate) fn into_uninit(self) -> super::UninitServer {
        let ServerExecutingState { conn, common, .. } = self.state;
        let conn = conn.update_state();
        let state = super::uninit::ServerUninitState { conn, common };
        debug!("moving client executing -> uninit");
        Machine::from_state(state)
    }

    async fn into_send_files_state(self) -> super::send_files::ServerSendFilesState {
        debug!(
            "moving {} server executing -> send files",
            self.state.common.node_name
        );
        let ServerExecutingState {
            conn,
            common,
            namespace,
            batch_name,
            job_identifier,
            job_name,
            save_location,
        } = self.state;

        #[allow(unused_mut)]
        let mut conn = conn.update_state();

        #[cfg(test)]
        assert!(conn.bytes_left().await == 0);

        super::send_files::ServerSendFilesState {
            conn,
            common,
            namespace,
            batch_name,
            job_identifier,
            job_name,
            save_location,
        }
    }

    async fn into_prepare_build_state(self) -> super::prepare_build::ServerPrepareBuildState {
        debug!(
            "moving {} server executing -> prepare build",
            self.state.common.node_name
        );
        let ServerExecutingState { conn, common, .. } = self.state;

        #[allow(unused_mut)]
        let mut conn = conn.update_state();

        #[cfg(test)]
        assert!(conn.bytes_left().await == 0);

        super::prepare_build::ServerPrepareBuildState { conn, common }
    }
}

#[derive(Serialize, Deserialize, Unwrap)]
pub(crate) enum ServerMsg {
    // passed to the client when a cancellation message was send to the client, but a
    // race condition has caused the client to miss
    OverrideWithCancellation,
    // confirm that the state the client is going to move to is correct, no need
    // to adjust for a cancellation
    Continue,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub(crate) enum ClientMsg {
    Cancelled,
    Successful,
    Failed,
}

impl ClientMsg {
    fn from_run_result(execution_output: Result<Option<()>, crate::Error>) -> Self {
        match execution_output {
            Ok(None) => Self::Cancelled,
            Ok(Some(_)) => Self::Successful,
            Err(_e) => Self::Failed,
        }
    }
}

impl transport::AssociatedMessage for ServerMsg {
    type Receive = ClientMsg;
}

impl transport::AssociatedMessage for ClientMsg {
    type Receive = ServerMsg;
}

struct CancelArbiter {
    // message channel to halt the arbiter
    stop_arbiter: oneshot::Sender<()>,
}

impl CancelArbiter {
    fn new(
        cancel_addr: SocketAddr,
        tx_cancel: broadcast::Sender<()>,
        is_cancelled: Arc<AtomicBool>,
    ) -> Self {
        let (tx, rx) = oneshot::channel();

        tokio::spawn(async move {
            tokio::select!(
                // the first branch of the selection here just returns when we receive a
                // message from the head node that the current job should be shutdown
                _ = client::return_on_cancellation(cancel_addr) => {
                    info!("cancelling job from arbiter");

                    // try to prevent any race conditions where the execution of the job finishes
                    is_cancelled.store(true, Ordering::Relaxed);

                    // ping the other process that we need to shutdown
                    tx_cancel.send(()).ok();
                }
                // the second branch returns when we are told that the main process has finished
                // the execution of the job, and we no longer need to monitor if this job is
                // getting cancelled now, we just need to shutdown
                //
                // the oneshot channel `rx` implements `Future` so we dont need any additional
                // methods
                _ = rx => {
                    // do nothing, the other branch of `select` will not be executed and this task
                    // will end
                }
            );
        });

        Self { stop_arbiter: tx }
    }

    /// stop the arbiter so it does not continue to monitor the port indefinitely
    async fn stop(self) {
        // its possible for a race condition to occur if we cancel the task fast enough,
        // lets just wait a short time just in case.
        tokio::time::sleep(Duration::from_millis(50)).await;

        // .ok() here since we may have already dropped the corresponding receiver
        // in the task since it was cancelled already
        self.stop_arbiter.send(()).ok();
    }
}

#[tokio::test]
/// ensure the cancel arbiter correctly ends the task and stores
/// an update to `is_cancelled`
async fn cancel_arbiter_positive() {
    //crate::logger();

    let is_cancelled = Arc::new(AtomicBool::new(false));
    let (tx_cancel, mut rx_cancel) = broadcast::channel(1);

    let port = 10_005;
    let cancel_addr = SocketAddr::from(([0, 0, 0, 0], port));

    let arbiter = CancelArbiter::new(cancel_addr, tx_cancel, Arc::clone(&is_cancelled));

    // allow tokio to bind to the port before making the request
    tokio::time::sleep(Duration::from_millis(200)).await;

    // make a connection to the port the arbiter is listening to
    let _conn = tokio::net::TcpStream::connect(&cancel_addr).await.unwrap();

    arbiter.stop().await;

    assert_eq!(is_cancelled.load(Ordering::Relaxed), true);

    // rx_cancel here simulates what the job-executioner would receive while it is executing the
    // job. This ensures that the job *will* get the signal to stop the execution as we expect
    assert_eq!(rx_cancel.try_recv().is_ok(), true);
}

#[tokio::test]
/// ensure the cancel arbiter does not send any signals that we dont expect
/// when it is cancelled without receiving a TCP connection to the cancel port
async fn cancel_arbiter_negative() {
    //crate::logger();

    let is_cancelled = Arc::new(AtomicBool::new(false));
    let (tx_cancel, mut rx_cancel) = broadcast::channel(1);

    let port = 10_006;
    let cancel_addr = SocketAddr::from(([0, 0, 0, 0], port));

    let arbiter = CancelArbiter::new(cancel_addr, tx_cancel, Arc::clone(&is_cancelled));

    // allow tokio to bind to the port before making the request
    tokio::time::sleep(Duration::from_millis(200)).await;

    arbiter.stop().await;

    // we did not send any cancel message, so is_cancelled should be false
    assert_eq!(is_cancelled.load(Ordering::Relaxed), false);

    // the job executioner should /not/ have a message, since we never actually sent one to the TCP
    // port
    assert_eq!(rx_cancel.try_recv().is_ok(), false);
}

#[tokio::test]
#[ignore]
async fn cancel_run() {
    let folder_path = PathBuf::from("./tests/python_sleep/");
    let file_to_execute = folder_path.join("sleep30s.py");

    let file_bytes = include_bytes!("../../tests/python_sleep/sleep30s.py");

    let work_dir = folder_path.join("run");

    assert_eq!(folder_path.exists(), true);
    assert_eq!(file_to_execute.exists(), true);

    // set up the distribute environment
    // this is a little frail depending on how APIs shift in the future
    client::utils::clean_output_dir(&work_dir).await.unwrap();

    let keepalive_port = 10_007;
    let transport_port = 10_008;
    let cancel_port = 10_009;

    let keepalive_addr = add_port(keepalive_port);
    let transport_addr = add_port(transport_port);
    let cancel_addr = add_port(cancel_port);

    //
    // setup TCP connections
    //
    let client_binding = tokio::net::TcpListener::bind(transport_addr).await.unwrap();

    let server_conn: transport::Connection<ServerMsg> =
        transport::Connection::new(transport_addr).await.unwrap();
    let client_conn: transport::Connection<ClientMsg> =
        transport::Connection::from_connection(client_binding.accept().await.unwrap().0);

    //
    // setup client state
    //
    let job = transport::JobOpt::from(transport::PythonJob {
        python_file: file_bytes.to_vec(),
        job_name: "test_job".into(),
        job_files: vec![],
    });

    let folder_state = client::execute::BindingFolderState::new();

    let client_state = ClientExecutingState {
        conn: client_conn,
        working_dir: work_dir.clone(),
        job,
        folder_state,
        cancel_addr,
    };

    //
    // setup server state
    //

    let (cancel_tx, common) =
        protocol::Common::test_configuration(transport_addr, keepalive_addr, cancel_addr);
    let job_identifier = server::JobIdentifier::Identity(1);

    let server_state = ServerExecutingState {
        conn: server_conn,
        common,
        namespace: "test_namespace".into(),
        batch_name: "test_batchname".into(),
        job_identifier,
        job_name: "test_name".into(),
        save_location: work_dir.join("server_backup"),
    };

    // setup each machine
    let client_machine = Machine::from_state(client_state);
    let server_machine = Machine::from_state(server_state);

    let (tx_client, rx_client) = oneshot::channel();
    let (tx_server, rx_server) = oneshot::channel();

    // spawn client proc
    tokio::spawn(async move {
        let next_client = client_machine.execute_job().await;
        tx_client.send(next_client).ok().unwrap();
    });

    // spawn server proc
    tokio::spawn(async move {
        let next_server = server_machine.wait_job_execution().await;
        tx_server.send(next_server).ok().unwrap();
    });

    // cancel the 30 second job after only 5 seconds
    tokio::time::sleep(Duration::from_secs(5)).await;
    cancel_tx.send(job_identifier).unwrap();

    let client = rx_client.await.unwrap().ok().unwrap();
    let server = rx_server.await.unwrap().ok().unwrap();

    // ensure both the client and server are in `PrepareBuild` state,
    // which implies that they were both cancelled
    assert_eq!(matches!(client, Either::Left(_)), true);
    assert_eq!(matches!(server, Either::Left(_)), true);

    // clean up the folder after
    assert_eq!(work_dir.exists(), true, "workdir does not exist somehow");
    std::fs::remove_dir_all(work_dir).unwrap();
}
