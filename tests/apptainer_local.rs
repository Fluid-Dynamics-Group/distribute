use distribute::cli::Run;

use std::fs;

use std::path::PathBuf;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn verify_apptainer_execution() {
    let dir = PathBuf::from("./tests/apptainer_local");

    assert_eq!(
        dir.join("apptainer_local.sif").exists(),
        true,
        "you need to run ./tests/apptainer_local/build.sh before executing tests"
    );

    let run = Run::new(dir.join("distribute-jobs.yaml"), dir.join("output"), true);

    distribute::run_local(run).await.unwrap();

    //let p1 = dir.join("output").join("_bind_path_0").join("file1.txt");
    //let p2 = dir.join("output").join("_bind_path_1").join("file2.txt");

    //dbg!(&p1);

    //assert_eq!(p1.exists(), true);
    //assert_eq!(p2.exists(), true);

    let output_1 = dir.join("output/archived_files/job_1/simulated_output.txt");
    let output_2 = dir.join("output/archived_files/job_2/simulated_output.txt");

    dbg!(&output_1);

    let data_1 = fs::read_to_string(output_1).unwrap();
    let data_2 = fs::read_to_string(output_2).unwrap();

    assert_eq!(data_1, "the square of the input was 100");
    assert_eq!(data_2, "the square of the input was 225");

    fs::remove_dir_all(dir.join("output")).unwrap();
}
