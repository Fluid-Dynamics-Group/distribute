pub(crate) mod execute;
pub(crate) mod utils;

pub(crate) use utils::WorkingDir;

pub(crate) use execute::{
    initialize_apptainer_job, initialize_python_job, run_apptainer_job, run_python_job,
    BindingFolderState,
};

use crate::error::Error;
use crate::prelude::*;
use crate::protocol;
use protocol::UninitClient;

use crate::{cli, error, transport};

pub async fn client_command(client: cli::Client) -> Result<(), Error> {
    let base_path = client.base_folder;
    let working_dir = WorkingDir::from(base_path.clone());

    working_dir
        .delete_and_create_folders()
        .await
        .map_err(error::ClientInitError::from)?;

    // ensure that `apptainer` was included in the path of the executable
    if let Err(e) = utils::verify_apptainer_in_path().await {
        error!(error = %e, "failed to verify that `apptainer` was in the $PATH environment variable - further execution will cause errors");
    }

    debug!(
        transport_port = client.transport_port,
        "starting transport port"
    );
    let addr = SocketAddr::from(([0, 0, 0, 0], client.transport_port));
    let listener = TcpListener::bind(addr)
        .await
        .map_err(error::TcpConnection::from)?;

    debug!(
        keepalive_port = client.keepalive_port,
        "starting keepalive port"
    );
    let keepalive_addr = SocketAddr::from(([0, 0, 0, 0], client.keepalive_port));
    start_keepalive_checker(keepalive_addr).await?;

    debug!(cancel_port = client.cancel_port, "starting cancel port");
    let cancel_addr = SocketAddr::from(([0, 0, 0, 0], client.cancel_port));

    info!("starting client listener on port {}", addr);
    println!("starting client listener on port {}", addr);

    loop {
        let (tcp_conn, _address) = listener
            .accept()
            .await
            .map_err(error::TcpConnection::from)?;
        println!("new TCP connection from the server - preparing setup / run structures - STDOUT");
        info!("new TCP connection from the server - preparing setup / run structures");

        run_job(tcp_conn, working_dir.clone(), cancel_addr)
            .await
            .ok();
    }

    #[allow(unreachable_code)]
    Ok(())
}

/// given a connection from the server, initialize a state machine and run as many jobs
/// through that connection as possible.
///
/// Only return from this function if there is a TcpConnection error
async fn run_job(
    conn: tokio::net::TcpStream,
    working_dir: WorkingDir,
    cancel_addr: SocketAddr,
) -> Result<(), ()> {
    let mut machine = protocol::Machine::<_, protocol::uninit::ClientUninitState>::new(
        conn,
        working_dir,
        cancel_addr,
    );

    debug!("created uninitialized state machine for running job on client");

    loop {
        match run_job_inner(machine).await {
            Ok(_) => {
                tracing::error!("hitting unreachable state - panicking");
                unreachable!("hitting unreachable state - panicking")
            }
            // error state if there was an issue with the TCP connection
            Err((_uninit, e)) if e.is_tcp_error() => {
                tracing::error!("TCP connection error from uninit detected, returning to main loop to accept new connection: {}", e);
                return Err(());
            }
            Err((_uninit, e)) => {
                tracing::error!("non TCP error when executing: {}", e);
                machine = _uninit;
            }
        }
    }
}

/// iterate over a [`protocol::PrepareBuildClient`]
/// and map it to _either_ a [`protocol::BuiltClient`]
/// or a  [`protocol::`](`protocol::PrepareBuildClient`)
async fn inner_prepare_build_to_compile_result(
    prepare_build: protocol::PrepareBuildClient,
) -> Result<
    protocol::Either<protocol::BuiltClient, protocol::PrepareBuildClient>,
    (UninitClient, protocol::ClientError),
> {
    let sending_compilation_files_state = match prepare_build.receive_job().await {
        Ok(send_files) => send_files,
        Err((prepare_build, err)) => {
            tracing::error!("error when trying to get a job to build : {}", err);
            return Err((prepare_build.into_uninit(), err.into()));
        }
    };

    // TODO: we can remove these by adding a new state to the send_files output that happens
    // after the job is executed. until then, we need this useless channel
    let (mut tx, _rx) = mpsc::channel(5);
    let building_state = match sending_compilation_files_state.receive_files(&mut tx).await {
        Ok(building) => building,
        Err((prepare_build, err)) => {
            tracing::error!(
                "error when trying to receive files that we will compile (send_files): {}",
                err
            );
            return Err((prepare_build.into_uninit(), err.into()));
        }
    };

    let building_state_or_prepare = match building_state.build_job().await {
        Ok(built) => built,
        Err((prepare_build, err)) => {
            tracing::error!("error when trying to get a job to build: {}", err);
            return Err((prepare_build.into_uninit(), err.into()));
        }
    };

    Ok(building_state_or_prepare)
}

/// map a [`protocol::PrepareBuildClient`] to a [`protocol::BuiltClient`] by recursively
/// calling [`inner_prepare_build_to_compile_result`]
async fn prepare_build_to_built(
    prepare_build: protocol::PrepareBuildClient,
) -> Result<protocol::BuiltClient, (UninitClient, protocol::ClientError)> {
    let mut building_state_or_prepare =
        inner_prepare_build_to_compile_result(prepare_build).await?;
    let built: protocol::BuiltClient;

    loop {
        // if we are here, we dont know if the return value from the compilation
        // in .build_job() was successfull or not. We need to check:
        match building_state_or_prepare {
            protocol::Either::Left(_built) => {
                built = _built;
                break;
            }
            protocol::Either::Right(prepare_build) => {
                building_state_or_prepare =
                    inner_prepare_build_to_compile_result(prepare_build).await?;
                continue;
            }
        };
    }

    Ok(built)
}

async fn execute_and_send_files(
    execute: protocol::ExecuteClient,
) -> Result<
    protocol::Either<protocol::PrepareBuildClient, protocol::BuiltClient>,
    (UninitClient, protocol::ClientError),
> {
    let send_files = match execute.execute_job().await {
        Ok(protocol::Either::Right(send)) => send,
        Ok(protocol::Either::Left(prepare_build)) => {
            return Ok(protocol::Either::Left(prepare_build))
        }
        Err((execute, err)) => {
            tracing::error!("error executing the job: {}", err);
            return Err((execute.into_uninit(), err.into()));
        }
    };

    let built = match send_files.send_files().await {
        Ok(built) => built,
        Err((send_files, err)) => {
            tracing::error!("error sending result files from the job: {}", err);
            return Err((send_files.into_uninit(), err.into()));
        }
    };

    Ok(protocol::Either::Right(built))
}

/// If there was a non-networking error, return the client in an uninitialized state. Otherwise, return
/// the networking error to the caller so they can handle the next steps. This is because
/// a network error will likely imply that the server will need to reconnect with us
async fn run_job_inner(uninit: UninitClient) -> Result<(), (UninitClient, protocol::ClientError)> {
    let prepare_build = match uninit.connect_to_host().await {
        Ok(prep) => prep,
        Err((uninit, err)) => {
            tracing::error!(
                "error when trying to connect from the uninit client: {}",
                err
            );
            return Err((uninit, err.into()));
        }
    };

    let mut built = prepare_build_to_built(prepare_build).await?;

    #[allow(unreachable_code)]
    loop {
        // now that we have compiled, we should
        let receive_files = match built.get_execute_instructions().await {
            Ok(protocol::Either::Right(executing)) => executing,
            Ok(protocol::Either::Left(prepare_build)) => {
                built = prepare_build_to_built(prepare_build).await?;
                continue;
            }
            Err((built, err)) => {
                tracing::error!("error from built client: {}", err);
                return Err((built.into_uninit(), err.into()));
            }
        };

        // TODO: we can remove these by adding a new state to the send_files output that happens
        // after the job is executed. until then, we need this useless channel
        let (mut tx, _rx) = mpsc::channel(5);
        let execute = match receive_files.receive_files(&mut tx).await {
            Ok(execute) => execute,
            Err((receive_files, err)) => {
                tracing::error!("error when trying to receive files to execute {}", err);
                return Err((receive_files.into_uninit(), err.into()));
            }
        };

        // fully execute the job and return back to the built state
        match execute_and_send_files(execute).await? {
            // we were probably cancelled while executing the job
            protocol::Either::Left(prepare_build) => {
                built = prepare_build_to_built(prepare_build).await?;
            }
            // we successfully finished the job, we are good to
            // move back to the built state
            protocol::Either::Right(_built) => {
                built = _built;
            }
        }
    }
}

/// start an arbiter to respoond to keepalive checks from the server
///
/// we do not need to communicate with the state of the nodes here - if
/// the node goes offline then the entire progrma will exit and this function
/// will cease to respond to the keepalive checks from the server.
pub(crate) async fn start_keepalive_checker(
    keepalive_port: SocketAddr,
) -> Result<(), error::TcpConnection> {
    info!("starting keepalive monitor on the client");
    // first, bind to the port so we can report errors before spawning off the task
    let listener = TcpListener::bind(keepalive_port)
        .await
        .map_err(error::TcpConnection::from)?;

    // spawn a task to monitor the port for keepalives
    tokio::spawn(async move {
        loop {
            let res = listener.accept().await;

            match res {
                Ok((conn, _addr)) => {
                    let mut connection = transport::Connection::from_connection(conn);
                    answer_query_connection(&mut connection).await.ok();
                }
                Err(e) => {
                    tracing::error!(
                        "error with client keepalive monitor while accepting a connection: {}",
                        e
                    );
                    continue;
                }
            }
        }
    });

    Ok(())
}

/// answer the server's message on the query port.
///
/// The message is a short query about what we are doing, including a keepalive
async fn answer_query_connection(
    client_conn: &mut transport::Connection<transport::ClientQueryAnswer>,
) -> Result<(), error::TcpConnection> {
    let query = client_conn.receive_data().await?;

    match query {
        transport::ServerQuery::KeepaliveCheck => {
            trace!("responded to keepalive connection");
            client_conn
                .transport_data(&transport::ClientQueryAnswer::KeepaliveResponse)
                .await
        }
        transport::ServerQuery::VersionCheck => {
            trace!("responded to version check connection");
            client_conn
                .transport_data(&transport::ClientQueryAnswer::VersionResponse(
                    transport::Version::current_version(),
                ))
                .await
        }
    }
}

fn kill_job(tx_cancel: &mut broadcast::Sender<()>) {
    // try to send out the cancelation order to all children.
    // since this function is called when there is an actively running
    // job task. However, there is probably a race condition in there somewhere
    // so we just ignore this possible error
    tx_cancel.send(()).ok();
}

/// check if the error from reading transport data from the tcp connection
/// was due to the connection being closed - and if so it means
/// that we should mark ourselves ready for additional jobs
///
/// returns true if the connection has been closed
fn is_closed_connection(error: error::TcpConnection) -> bool {
    // TODO: experiment with what exactly is the EOF on a TCP connection
    // and what exactly constitutes waiting for more data
    matches!(error, error::TcpConnection::ConnectionClosed)
}

pub(crate) async fn check_for_failure<ClientMsg, ServerMsg, F>(
    conn: &mut transport::Connection<ClientMsg>,
    is_cancellation: F,
    send_on_cancel: broadcast::Sender<()>,
    did_cancel: &mut bool,
) -> !
where
    ClientMsg: transport::AssociatedMessage<Receive = ServerMsg> + Serialize,
    ServerMsg: serde::de::DeserializeOwned + std::fmt::Debug,
    F: Fn(&ServerMsg) -> bool,
{
    loop {
        let rx: Result<ServerMsg, _> = conn.receive_data().await;
        if let Ok(msg) = rx {
            if is_cancellation(&msg) {
                *did_cancel = true;
                send_on_cancel.send(()).ok();
            } else {
                warn!("received non-cancel message from the server: {:?}", msg);
            }
        }
    }
}

/// binds to cancellation port and waits for a connection to the port. After receiving a
/// connection, this will contain a cancellation request from the head node dictating that our
/// current job ID matches a request to kill the jobs.
pub(crate) async fn return_on_cancellation(addr: SocketAddr) -> Result<(), error::TcpConnection> {
    let listener = TcpListener::bind(addr)
        .await
        .map_err(error::TcpConnection::from)?;

    loop {
        // accept the connection
        let res = listener.accept().await;
        debug!("accepted connection on cancellation port");

        match res {
            Ok(_) => {
                return Ok(());
            }
            Err(e) => tracing::error!("error when accepting cancellation connection: {e}"),
        }
    }
}
