use distribute::cli::Run;
use distribute::cli::Slurm;

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

#[test]
fn slurm_output_verify() {
    if false {
        distribute::logger();
    }

    let dir = PathBuf::from("./tests/apptainer_local");

    assert_eq!(
        dir.join("apptainer_local.sif").exists(),
        true,
        "you need to run ./tests/apptainer_local/build.sh before executing tests"
    );

    let output_dir = dir.join("slurm_output");

    // clean out the previous output directory if it still exists
    fs::remove_dir_all(&output_dir).ok();

    let cluster_username = "bkarlik".into();
    let cluster_address = "pronghorn.rc.unr.edu".into();
    let cluser_destination = "/data/gpfs/home/bkarlik".into();

    let slurm_command = Slurm::new(
        output_dir.clone(),
        dir.join("distribute-jobs.yaml"),
        cluster_username,
        cluster_address,
        cluser_destination,
    );

    distribute::slurm(slurm_command).unwrap();

    let task1_dir = output_dir.join("first_job");
    let task2_dir = output_dir.join("job_2");

    assert!(output_dir.join("apptainer.sif").exists());

    // output should look like this
    //
    // ├── apptainer.sif
    // ├── first_job
    // │   ├── input
    // │   │   ├── hello.txt -> ../../input/hello.txt
    // │   │   └── input.txt
    // │   ├── mnt_00
    // │   ├── mnt_01
    // │   ├── output
    // │   └── slurm_input.sl
    // ├── input
    // │   └── hello.txt
    // └── job_2
    //     ├── input
    //     │   ├── hello.txt -> ../../input/hello.txt
    //     │   └── input.txt
    //     ├── mnt_00
    //     ├── mnt_01
    //     ├── output
    //     └── slurm_input.sl

    verify_basic_output(&task1_dir);
    verify_basic_output(&task2_dir);
}

fn verify_basic_output(task_dir: &std::path::Path) {
    assert!(task_dir.exists());

    let mnt_1 = task_dir.join("mnt_00");
    let mnt_2 = task_dir.join("mnt_01");
    let output = task_dir.join("output");
    let input = task_dir.join("input");

    for path in [&mnt_1, &mnt_2, &output, &input] {
        assert!(path.exists());
    }

    for file in ["hello.txt", "input.txt"] {
        assert!(input.join(file).exists());
    }
}
