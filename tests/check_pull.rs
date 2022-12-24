use distribute::cli::Pull;
use distribute::cli::Server;

use std::fs;
use std::io::Write;
use std::net::IpAddr;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn check_pull() {
    if false {
        distribute::logger();
    }

    let server_port = 9980;
    let addr: IpAddr = [0, 0, 0, 0].into();

    let dir: PathBuf = "./tests/check_pull/".into();
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

    let dry = false;
    let skip_folders = false;

    // initialize the pull and server commands as they would be read from the CLI

    let pull = Pull::new(
        addr,
        dir.join("distribute-jobs.yaml"),
        dry,
        skip_folders,
        server_port,
        save_dir.clone(),
        None,
    );
    let server = Server::new(
        nodes_file,
        server_path.clone(),
        server_temp.clone(),
        server_port,
        false,
        None,
    );

    dbg!(&server);

    // start the server
    tokio::task::spawn(async move {
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
    assert_eq!(
        dbg!(out_namespace.join("job_1").join("file1.txt")).exists(),
        true
    );
    assert_eq!(out_namespace.join("job_2").join("file2.txt").exists(), true);
    assert_eq!(out_namespace.join("job_3").join("file3.txt").exists(), true);

    // cleaup remaining files
    fs::remove_dir_all(&save_dir).ok();
    fs::remove_dir_all(&server_path).ok();
    fs::remove_dir_all(&server_temp).ok();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pull_large_file() {
    if false {
        distribute::logger();
    }

    let server_port = 9981;
    let addr: IpAddr = [0, 0, 0, 0].into();

    let dir: PathBuf = "./tests/pull_large_file/".into();
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

    fs::create_dir(&j1).ok();

    // write a file to each of the directories
    let j1_1 = j1.join("file1.txt");

    let length = 10.01f64.powf(9.) as usize;
    dbg!(length);
    fs::File::create(&j1_1)
        .unwrap()
        .write_all(&vec![0; length])
        .unwrap();

    assert!(
        fs::metadata(&j1_1).unwrap().len() == length as u64,
        "file length was not the expected length"
    );
    dbg!(fs::metadata(&j1_1).unwrap().len());

    // initialize the pull and server commands as they would be read from the CLI
    let dry = false;
    let skip_folders = false;

    let pull = Pull::new(
        addr,
        dir.join("distribute-jobs.yaml"),
        dry,
        skip_folders,
        server_port,
        save_dir.clone(),
        None,
    );
    let server = Server::new(
        nodes_file,
        server_path.clone(),
        server_temp.clone(),
        server_port,
        false,
        None,
    );

    dbg!(&server);

    // start the server
    tokio::task::spawn(async move {
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
    let f1_out = out_namespace.join("job_1").join("file1.txt");
    assert_eq!(dbg!(&f1_out).exists(), true);

    assert_eq!(fs::metadata(&f1_out).unwrap().len(), length as u64);

    // cleaup remaining files
    fs::remove_dir_all(&save_dir).ok();
    fs::remove_dir_all(&server_path).ok();
    fs::remove_dir_all(&server_temp).ok();
}
