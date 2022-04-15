/// Verify that jobs are removed from the job queue after they are completed
/// and will therefore notify the end user about the completion
use distribute::cli::Add;
use distribute::cli::Client;
use distribute::cli::Server;
use distribute::cli::Status;

use std::fs;
use std::net::IpAddr;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
async fn check_deallocate_jobs() {
    if true {
        distribute::logger();
    }
    let server_port = 9981;
    let addr: IpAddr = [0, 0, 0, 0].into();

    let dir: PathBuf = "./tests/check_deallocate_jobs/".into();
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
    let client = Client::new(client_workdir.clone(), 9967, "./output.log".into());
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
    );

    // start the server
    tokio::spawn(async move {
        println!("starting server");
        distribute::server_command(server).await.unwrap();
        println!("server has exited");
    });

    // let the server start up for a few seconds
    thread::sleep(Duration::from_secs(1));

    // configure a job to send off to the server
    let run = Add::new(
        "./tests/apptainer_local/distribute-jobs.yaml".into(),
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

    let status = Status::new(server_port, addr);
    let jobs = distribute::get_current_jobs(&status).await.unwrap();
    assert!(jobs.len() == 1);

    thread::sleep(Duration::from_secs(30));

    let status = Status::new(server_port, addr);
    let jobs = distribute::get_current_jobs(&status).await.unwrap();

    dbg!(&jobs);

    assert_eq!(jobs.len(), 0);

    //directory tree should be this:
    // check_deallocate_jobs
    //     ├── distribute-nodes.yaml
    //     ├── server_save_dir
    //     │   └── some_namespace
    //     │       └── some_batch
    //     │           ├── job_1
    //     │           │   ├── job_1_output.txt
    //     │           │   └── simulated_output.txt
    //     │           └── job_2
    //     │               ├── job_2_output.txt
    //     │               └── simulated_output.txt

    let batch = server_save_dir.join("some_namespace/some_batch");
    assert_eq!(batch.join("job_1/simulated_output.txt").exists(), true);
    assert_eq!(batch.join("job_2/simulated_output.txt").exists(), true);

    fs::remove_dir_all(&server_save_dir).ok();
    fs::remove_dir_all(&server_temp_dir).ok();
    fs::remove_dir_all(&client_workdir).ok();
}
