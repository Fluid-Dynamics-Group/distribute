pub(crate) mod execute;
pub(crate) mod utils;

pub(crate) use execute::{
    initialize_python_job, initialize_singularity_job, run_python_job, run_singularity_job,
    BindingFolderState,
};

use crate::error::Error;
use crate::prelude::*;
use crate::protocol;
use protocol::UninitClient;

use crate::{cli, error, transport};

use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;

pub async fn client_command(client: cli::Client) -> Result<(), Error> {
    let base_path = PathBuf::from(client.base_folder);
    utils::clean_output_dir(&base_path)
        .await
        .map_err(error::ClientInitError::from)?;

    let listener = TcpListener::bind(SocketAddr::from(([0, 0, 0, 0], client.port)))
        .await
        .map_err(error::TcpConnection::from)?;

    loop {
        let (tcp_conn, _address) = listener
            .accept()
            .await
            .map_err(error::TcpConnection::from)?;

        run_job(tcp_conn, &base_path).await.ok();
    }

    #[allow(unreachable_code)]
    Ok(())
}

/// given a connection from the server, initialize a state machine and run as many jobs
/// through that connection as possible.
///
/// Only return from this function if there is a TcpConnection error
async fn run_job(conn: tokio::net::TcpStream, working_dir: &Path) -> Result<(), ()> {
    let mut machine = protocol::Machine::<_, protocol::uninit::ClientUninitState>::new(conn, working_dir.to_owned());

    loop {
        match run_job_inner(machine).await {
            Ok(_) => unreachable!(),
            // error state if there was an issue with the TCP connection
            Err((_uninit, e)) if e.is_tcp_error() => {
                error!("TCP connection error from uninit detected, returning to main loop to accept new connection: {}", e);
                //return Err(conn);
                return Err(());
            }
            Err((_uninit, e)) => {
                error!("non TCP error when executing: {}", e);
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
    let building_state = match prepare_build.receive_job().await {
        Ok(building) => building,
        Err((prepare_build, err)) => {
            error!("error when trying to get a job to build : {}", err);
            return Err((prepare_build.to_uninit(), err.into()));
        }
    };

    let building_state_or_prepare = match building_state.build_job().await {
        Ok(built) => built,
        Err((prepare_build, err)) => {
            error!("error when trying to get a job to build: {}", err);
            return Err((prepare_build.to_uninit(), err.into()));
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
            error!("error executing the job: {}", err);
            return Err((execute.to_uninit(), err.into()));
        }
    };

    let built = match send_files.send_files().await {
        Ok(built) => built,
        Err((send_files, err)) => {
            error!("error sending result files from the job: {}", err);
            return Err((send_files.to_uninit(), err.into()));
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
            error!(
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
        let execute = match built.get_execute_instructions().await {
            Ok(protocol::Either::Right(executing)) => executing,
            Ok(protocol::Either::Left(prepare_build)) => {
                built = prepare_build_to_built(prepare_build).await?;
                continue;
            }
            Err((built, err)) => {
                error!("error from built client: {}", err);
                return Err((built.to_uninit(), err.into()));
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

/// answer the server's message on the query port.
///
/// The message is a short query about what we are doing, including a keepalive
async fn answer_query_connection(
    client_conn: transport::Connection<transport::ClientQueryAnswer>,
) -> Result<(), error::TcpConnection> {
    todo!()
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
    match error {
        // TODO: experiment with what exactly is the EOF on a TCP connection
        // and what exactly constitutes waiting for more data
        error::TcpConnection::ConnectionClosed => true,
        _ => false,
    }
}
