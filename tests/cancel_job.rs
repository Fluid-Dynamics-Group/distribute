use distribute::cli::Add;
use distribute::cli::Client;
/// cancel a simple set of python python jobs and ensure that
/// the `currently running jobs` counter on the scheduler
/// is decremented as we would expect
use distribute::cli::Kill;
use distribute::cli::Server;
use distribute::cli::ServerStatus;

use std::fs;
use std::net::IpAddr;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
async fn cancel_job() {
    println!("starting cancel job test");
    if false {
        distribute::logger();
    }

    let server_port = 9981;
    // this is the port in the corresponding distribute-nodes.yaml file for this job
    let client_port = 9967;
    let keepalive_port = 9968;
    let cancel_port = 8955;
    let addr: IpAddr = [0, 0, 0, 0].into();

    let dir: PathBuf = "./tests/cancel_job/".into();
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
    let client = Client::new(
        client_workdir.clone(),
        client_port,
        keepalive_port,
        cancel_port,
        "./output.log".into(),
    );
    tokio::spawn(async move {
        println!("starting the client");
        distribute::client_command(client).await.unwrap();
        println!("client has exited");
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

    thread::sleep(Duration::from_secs(30));

    //
    // ensure the job set is running right now
    //
    let status = ServerStatus::new(server_port, addr);
    let jobs = distribute::get_current_jobs(&status).await.unwrap();

    assert!(jobs[0].running_jobs.len() == 1);

    //
    // cancel the job set then
    //
    // `job_name` comes from distribute-jobs.yaml file
    let kill_msg = Kill {
        port: server_port,
        ip: addr,
        job_name: "python_sleep_job".into(),
    };
    distribute::kill(kill_msg).await.unwrap();

    // sleep a bit after the notice to cancel the jobs to give
    // the message queues ample time
    thread::sleep(Duration::from_secs(5));

    // check how many jobs are left
    let status = ServerStatus::new(server_port, addr);
    let jobs = distribute::get_current_jobs(&status).await.unwrap();

    dbg!(&jobs);
    dbg!(&jobs.len());

    assert_eq!(jobs.len(), 0);

    fs::remove_dir_all(&server_save_dir).ok();
    fs::remove_dir_all(&server_temp_dir).ok();
    fs::remove_dir_all(&client_workdir).ok();
}
