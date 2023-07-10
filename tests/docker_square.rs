/// Baseline integration test for docker behavior
use distribute::cli::Add;
use distribute::cli::Client;
use distribute::cli::Server;
use distribute::cli::ServerStatus;

use std::fs;
use std::net::IpAddr;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
async fn docker_square() {
    if false {
        distribute::logger();
    }

    // TODO: check the docker file exists before  executing
    assert_eq!(
        PathBuf::from("./tests/apptainer_local/apptainer_local.sif").exists(),
        true,
        "you need to run ./tests/apptainer_local/build.sh before executing tests"
    );

    let server_port = 9981;
    // this is the port in the corresponding distribute-nodes.yaml file for this job
    let client_port = 9967;
    let keepalive_port = 9968;
    let cancel_port = 9969;
    let addr: IpAddr = [0, 0, 0, 0].into();

    let dir: PathBuf = "./tests/docker_square/".into();
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
        "./tests/docker_square/distribute-jobs.yaml".into(),
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
    assert!(jobs.len() == 1);

    thread::sleep(Duration::from_secs(15));

    let status = ServerStatus::new(server_port, addr);
    let jobs = distribute::get_current_jobs(&status).await.unwrap();

    dbg!(&jobs);
    dbg!(&jobs.len());

    // in the vector of all the job sets added (1 at most in length),
    // we should have no jobs set information remaining because all the jobs
    // were deallocated
    assert_eq!(jobs.len(), 0);

    // directory tree should be this:
    // docker_square
    // ├── server_save_dir
    // │   └── some_namespace
    // │       └── some_batch
    // │           ├── job_1
    // │           │   ├── job_1_output.txt
    // │           │   └── output.txt
    // │           └── job_2
    // │               ├── job_2_output.txt
    // │               └── output.txt


    let batch = server_save_dir.join("some_namespace/some_batch");
    assert_eq!(
        batch.join("job_1/output.txt").exists(),
        true,
        "missing job 1 simulation output"
    );
    assert_eq!(
        batch.join("job_2/output.txt").exists(),
        true,
        "missing job 2 simulation output"
    );

    // we should also have output files for the jobs that we ran
    assert_eq!(
        batch.join("job_1/job_1_output.txt").exists(),
        true,
        "missing job 1 STDOUT file"
    );
    assert_eq!(
        batch.join("job_2/job_2_output.txt").exists(),
        true,
        "missing job 2 STDOUT file"
    );

    fs::remove_dir_all(&server_save_dir).ok();
    fs::remove_dir_all(&server_temp_dir).ok();
    fs::remove_dir_all(&client_workdir).ok();
}

