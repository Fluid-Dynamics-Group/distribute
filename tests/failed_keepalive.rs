/// simulate a node going offline and ensure that jobs are properly
/// added back to the queue
use distribute::cli::Add;
use distribute::cli::Client;
use distribute::cli::Server;
use distribute::cli::ServerStatus;

use std::fs;
use std::net::IpAddr;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use tracing::{error, info};

#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
async fn failed_keepalive() {
    println!("starting failed keepalive test");
    if false {
        distribute::logger();
    }

    let current_dir = std::env::current_dir().unwrap();

    let server_port = 9981;
    // this is the port in the corresponding distribute-nodes.yaml file for this job
    let client_port = 9967;
    let keepalive_port = 9968;
    let cancel_port = 8955;
    let addr: IpAddr = [0, 0, 0, 0].into();

    let dir: PathBuf = "./tests/failed_keepalive/".into();
    let nodes_file: PathBuf = dir.join("distribute-nodes.yaml");
    let server_save_dir = dir.join("server_save_dir");
    let server_temp_dir = dir.join("server_temp_dir");
    let client_workdir = dir.join("workdir");

    fs::remove_dir_all(&server_save_dir).ok();
    fs::remove_dir_all(&server_temp_dir).ok();
    fs::remove_dir_all(&client_workdir).ok();

    fs::create_dir(&server_save_dir).unwrap();
    fs::create_dir(&server_temp_dir).unwrap();
    fs::create_dir(&client_workdir).unwrap();

    // start up a client
    // the port comes from distribute-nodes.yaml

    let client_failure_time = std::time::Duration::from_secs(30);

    let client = Client::new(
        client_workdir.clone(),
        client_port,
        keepalive_port,
        cancel_port,
        "./output.log".into(),
    );
    tokio::spawn(async move {
        println!("starting the client");
        let fut = distribute::client_command(client);
        let res = tokio::time::timeout(client_failure_time, fut).await;
        if let Err(_) = res {
            println!("client has been knocked offline after a timeout, keepalive should now fail");
            info!("client has been knocked offline after a timeout, keepalive should now fail");

            // Since we artificially killed the client (simulating a shutdown of the computer)
            // since these tasks share environment variables, the client has not cleaned up their
            // execution model, and therefore the python script is still in the wrong directory
            //
            // here, we just set the directory to the correct value
            //
            // we normally would not need to do this since the client and server would be
            // separate programs with separate environment variables (also likely on
            // separate computers)
            std::env::set_current_dir(&current_dir).unwrap();
        } else {
            error!("client did not fail keepalive!");
            panic!("client did not fail keepalive!")
        }
    });

    thread::sleep(Duration::from_secs(1));

    let server = Server::new(
        nodes_file,
        server_save_dir.clone(),
        server_temp_dir.clone(),
        server_port,
        false,
        None,
    );

    // start the server
    tokio::task::spawn(async move {
        println!("starting server");
        distribute::server_command(server).await.unwrap();
        println!("server has exited");
    });

    // let the server start up for a few seconds
    thread::sleep(Duration::from_secs(1));

    // configure a job to send off to the server
    let run = Add::new(
        dir.join("distribute-jobs.yaml").into(),
        server_port,
        addr,
        false,
        false,
    );
    distribute::add(run).await.unwrap();

    // we know that it takes around 10 seconds for a job to get scheduled to
    // a node from the sleep section is src/node.rs - therefore we wait:
    // 10 seconds - job to get scheduled
    // 10 seconds - file to get sent
    // 5 seconds - all jobs to finish

    let status = ServerStatus::new(server_port, addr);
    let jobs = distribute::get_current_jobs(&status).await.unwrap();

    // we should have 1 job set on the server currently
    // this job set will have multiple jobs within it
    assert!(jobs.len() == 1);

    // sleep until after the client has been knocked offline.
    // at this point, the correct behavior is for the client
    // to return the job to the pool and decrement the total
    // running job counter to 0
    thread::sleep(client_failure_time + Duration::from_secs(5));

    //
    // ensure the job set is running right now
    //
    let status = ServerStatus::new(server_port, addr);
    let jobs = distribute::get_current_jobs(&status).await.unwrap();

    dbg!(&jobs);

    assert!(jobs[0].running_jobs.len() == 0);
    assert!(jobs[0].jobs_left.len() == 1);

    fs::remove_dir_all(&server_save_dir).ok();
    fs::remove_dir_all(&server_temp_dir).ok();
    fs::remove_dir_all(&client_workdir).ok();
}
