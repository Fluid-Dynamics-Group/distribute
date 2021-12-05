use distribute::cli::Pull;
use distribute::cli::Server;

use std::path::PathBuf;
use std::net::IpAddr;
use std::fs;
use std::time::Duration;
use std::thread;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn other() {
    if false {
        logger();
    }

    let server_port = 9980;
    let addr : IpAddr = [0, 0, 0, 0].into();

    let dir : PathBuf = "./tests/check_pull/".into();
    let save_dir = dir.join("destination_dir");

    // server stuff
    let nodes_file = dir.join("distribute-nodes.yaml");
    let server_path = dir.join("server_save");
    let server_temp = dir.join("server_temp");

    fs::remove_dir_all(&save_dir).ok();
    fs::remove_dir_all(&server_path).ok();
    fs::remove_dir_all(&server_temp).ok();

    fs::create_dir(&save_dir).ok();
    fs::create_dir(&server_path).ok();
    fs::create_dir(&server_temp).ok();

    // setup a namespace directory that we can write things to
    let namespace_name = "some_namespace/some_batch";
    let namespace_path = server_path.join(namespace_name);
    fs::remove_dir_all(&namespace_path).ok();
    fs::create_dir_all(&namespace_path).unwrap();

    // create some directories to work with
    let j1 = namespace_path.join("job_1");
    let j2 = namespace_path.join("job_2");
    let j3 = namespace_path.join("job_3");

    fs::create_dir(&j1).ok();
    fs::create_dir(&j2).ok();
    fs::create_dir(&j3).ok();

    // write a file to each of the directories
    let j1_1 = j1.join("file1.txt");
    let j2_1 = j2.join("file2.txt");
    let j3_1 = j3.join("file3.txt");

    fs::File::create(j1_1).unwrap();
    fs::File::create(j2_1).unwrap();
    fs::File::create(j3_1).unwrap();

    // initialize the pull and server commands as they would be read from the CLI

    let pull = Pull::new(addr, dir.join("distribute-jobs.yaml"), false, server_port, save_dir.clone(), None);
    let server = Server::new(nodes_file.to_string_lossy().to_string(), server_path.clone(), server_temp.clone(), server_port, false);

    dbg!(&server);
    
    // start the server
    tokio::spawn(async move {
        println!("starting server");
        distribute::server_command(server).await.unwrap();
        println!("server has exited");
    });

    // let the server start up for a few seconds
    thread::sleep(Duration::from_secs(3));

    // execute the pulling command
    distribute::pull(pull).await.unwrap();

    // make sure that all the paths exist - we dont include the namespace in the output
    let out_namespace = save_dir.join("some_batch");
    assert_eq!(dbg!(out_namespace.join("job_1").join("file1.txt")).exists(), true);
    assert_eq!(out_namespace.join("job_2").join("file2.txt").exists(), true);
    assert_eq!(out_namespace.join("job_3").join("file3.txt").exists(), true);
    
    // cleaup remaining files
    fs::remove_dir_all(&save_dir).ok();
    fs::remove_dir_all(&server_path).ok();
    fs::remove_dir_all(&server_temp).ok();
}

fn logger() {
    fern::Dispatch::new()
        // Perform allocation-free log formatting
        .format(|out, message, record| {
            out.finish(format_args!(
                "{}[{}][{}] {}",
                chrono::Local::now().format("[%Y-%m-%d][%H:%M:%S]"),
                record.target(),
                record.level(),
                message
            ))
        })
        // Add blanket level filter -
        .level(log::LevelFilter::Debug)
        // - and per-module overrides
        .level_for("hyper", log::LevelFilter::Info)
        // Output to stdout, files, and other Dispatch configurations
        .chain(std::io::stdout())
        // Apply globally
        .apply()
        .map_err(distribute::LogError::from).unwrap();
}