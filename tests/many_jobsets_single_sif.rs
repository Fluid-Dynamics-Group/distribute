use distribute::cli::Run;
use distribute::cli::Slurm;

use distribute::cli::Add;
use distribute::cli::Client;
use distribute::cli::Server;
use distribute::cli::ServerStatus;

use std::fs;
use std::net::IpAddr;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn many_jobsets_single_sif() {
    println!("starting many_jobsets_single_sif");
    if false {
        distribute::logger();
    }

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

    let dir: PathBuf = "./tests/many_jobsets_single_sif".into();
    let nodes_file: PathBuf = dir.join("distribute-nodes.yaml");
    let server_save_dir = dir.join("server_save_dir");
    let server_temp_dir = dir.join("server_temp_dir");
    let client_workdir = dir.join("workdir");
    let configs_dir = dir.join("configs");

    fs::remove_dir_all(&server_save_dir).ok();
    fs::remove_dir_all(&server_temp_dir).ok();
    fs::remove_dir_all(&client_workdir).ok();
    fs::remove_dir_all(&configs_dir).ok();

    fs::create_dir(&server_save_dir).unwrap();
    fs::create_dir(&server_temp_dir).unwrap();
    fs::create_dir(&client_workdir).unwrap();
    fs::create_dir(&configs_dir).unwrap();

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

    let num_batches = 10;

    let config_paths = write_many_batch_configs(
        "./tests/apptainer_local/distribute-jobs.yaml".into(),
        configs_dir.clone(),
        num_batches,
    );

    dbg!(&config_paths);

    //
    // Add a bunch of jobs to the server, all using the same sif file
    //

    for config_path in &config_paths {
        // configure a job to send off to the server
        let run = Add::new(
            config_path.to_owned(),
            server_port,
            addr,
            false,
            false,
        );
        distribute::add(run).await.unwrap();
    }

    // we know that it takes around 10 seconds for a job to get scheduled to
    // a node from the sleep section is src/node.rs - therefore we wait:
    // 10 seconds - job to get scheduled
    // 10 seconds - file to get sent
    // 5 * num_batches  seconds - all jobs to finish

    //let status = ServerStatus::new(server_port, addr);
    //let jobs = distribute::get_current_jobs(&status).await.unwrap();

    // check that all batches are available on the server 
    //dbg!(jobs.len());
    //dbg!(&jobs);
    //assert!(jobs.len() == num_batches);

    thread::sleep(Duration::from_secs(10 + 10 + 5 * (num_batches as u64)));

    let status = ServerStatus::new(server_port, addr);
    let jobs = distribute::get_current_jobs(&status).await.unwrap();

    // print out some logs on what jobs are still not scheduled
    distribute::server_status(status).await.unwrap();

    dbg!(&jobs);
    dbg!(&jobs.len());

    // in the vector of all the job sets added (1 at most in length),
    // we should have no jobs set information remaining because all the jobs
    // were deallocated
    assert_eq!(jobs.len(), 0);

    //// directory tree should be this:
    //// check_deallocate_jobs
    ////     ├── distribute-nodes.yaml
    ////     ├── server_save_dir
    ////     │   └── some_namespace
    ////     │       └── some_batch
    ////     │           ├── job_1
    ////     │           │   ├── job_1_output.txt
    ////     │           │   └── simulated_output.txt
    ////     │           └── job_2
    ////     │               ├── job_2_output.txt
    ////     │               └── simulated_output.txt

    //let batch = server_save_dir.join("some_namespace/some_batch");
    //assert_eq!(
    //    batch.join("job_1/simulated_output.txt").exists(),
    //    true,
    //    "missing job 1 simulation output"
    //);
    //assert_eq!(
    //    batch.join("job_2/simulated_output.txt").exists(),
    //    true,
    //    "missing job 2 simulation output"
    //);

    //// we should also have output files for the jobs that we ran
    //assert_eq!(
    //    batch.join("job_1/job_1_output.txt").exists(),
    //    true,
    //    "missing job 1 output file"
    //);
    //assert_eq!(
    //    batch.join("job_2/job_2_output.txt").exists(),
    //    true,
    //    "missing job 2 output file"
    //);

    //// we should not have output files from jobs we did not run
    //assert_eq!(
    //    batch.join("job_1/job_2_output.txt").exists(),
    //    false,
    //    "output for job 2 exists in job 1"
    //);
    //assert_eq!(
    //    batch.join("job_2/job_1_output.txt").exists(),
    //    false,
    //    "output for job 1 exists in job 2"
    //);

    fs::remove_dir_all(&server_save_dir).ok();
    fs::remove_dir_all(&server_temp_dir).ok();
    fs::remove_dir_all(&client_workdir).ok();
    fs::remove_dir_all(&configs_dir).ok();
}

/// Generate a bunch of config files based on a input config file identical
/// to the template w/ a change of the batch name
fn write_many_batch_configs(
    base_config: PathBuf,
    config_dir: PathBuf,
    num_configs: usize,
) -> Vec<PathBuf> {
    use distribute::NormalizePaths;

    let mut out = Vec::new();

    let relative_path = PathBuf::from("./tests/apptainer_local/").canonicalize().unwrap();

    let mut main_config =
        distribute::load_config::<distribute::Jobs<distribute::common::File>>(&base_config, false)
            .unwrap();

    main_config.normalize_paths(relative_path);

    let config: distribute::ApptainerConfig<_> = match main_config {
        distribute::Jobs::Python(_) => unreachable!(),
        distribute::Jobs::Apptainer(app) => app,
    };

    for i in 0..num_configs {
        let config_name = format!("distribute-jobs-{i:02}.yaml");
        let config_path = config_dir.join(config_name);
        let file = fs::File::create(&config_path).unwrap();
        let mut cfg = config.clone();

        cfg.meta_mut().set_batch_name(format!("batch_{i:02}"));
        cfg.to_writer(file).unwrap();

        out.push(config_path);
    }

    out
}
