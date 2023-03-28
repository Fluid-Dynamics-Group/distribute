# Running Locally / Debugging Apptainer Jobs

Because there are a lot of ways that your job might go wrong, you can use the `distribute run` command 
to run an apptainer configuration file in place. As an example, take [this test](https://github.com/Fluid-Dynamics-Group/distribute/tree/cancel-tasks/tests/apptainer_local)
that is required to compile and run in the project. The apptainer definition file is:


```
Bootstrap: library
From: ubuntu:20.04

%files
	./run.py /run.py

%post
	apt-get update -y
	apt install python3 -y

%apprun distribute
    cd /
    python3 /run.py $1
```

`run.py` is:

```python
import sys

def main():
    procs = int(sys.argv[1])
    print(f"running with {procs} processors")

    print("writing to /dir1")
    with open("/dir1/file1.txt", "w") as f:
        f.write("checking mutability of file system")

    print("writing to /dir2")
    with open("/dir2/file2.txt", "w") as f:
        f.write("checking mutability of file system")

    # read some input files from /input

    print("reading input files")
    with open("/input/input.txt", "r") as f:
        text = f.read()
        num = int(text)

    with open("/distribute_save/simulated_output.txt", "w") as f:
        square = num * num
        f.write(f"the square of the input was {square}")

if __name__ == "__main__":
    main()
```

`input_1.txt` is:

```
10
```

`input_2.txt` is:

```
15
```

and `distribute-jobs.yaml` is:


```yaml
---
meta:
  batch_name: some_batch
  namespace: some_namespace
  capabilities: []
apptainer:
  initialize:
    sif: apptainer_local.sif
    required_files: []
    required_mounts:
      - /dir1
      - /dir2
  jobs:
    - name: job_1
      required_files:
        - path: input_1.txt
          alias: input.txt
    - name: job_2
      required_files:
        - path: input_2.txt
          alias: input.txt
```

the apptainer definition file can be built with [these instructions](https://github.com/Fluid-Dynamics-Group/distribute/blob/cancel-tasks/tests/apptainer_local/build.sh).
Then, execute the job locally:

```
distribute run distribute-jobs.yaml --save-dir output --clean-save
```

The output directory structure looks like this:

```
output
├── archived_files
│   ├── job_1
│   │   ├── job_1_output.txt
│   │   └── simulated_output.txt
│   └── job_2
│       ├── job_2_output.txt
│       └── simulated_output.txt
├── _bind_path_0
│   └── file1.txt
├── _bind_path_1
│   └── file2.txt
├── distribute_save
├── initial_files
├── input
│   └── input.txt
└── apptainer_file.sif
```

This shows that we were able to write to additional folders on the host system (`_bind_path_x`), as well as read and write output files. Its worth noting that 
if this job was run on the distributed server, it would not be archived the same (`archive_files` directory is simply a way to save `distribute_save` without
deleting data). The structure on the server would look like this:

```
some_namespace
├── some_batch
    ├── job_1
    │   ├── job_1_output.txt
    │   └── simulated_output.txt
    └── job_2
        ├── job_2_output.txt
        └── simulated_output.txt
```

The outputs of the two `simulated_output.txt` files are:

```
the square of the input was 100
```

and

```
the square of the input was 225
```
