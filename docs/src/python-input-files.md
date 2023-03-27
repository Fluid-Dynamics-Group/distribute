# Input Files

You may ask, what do your see when they are executed on a node? While the base folder structure remains the same,
the files you are provided differ. Lets say you are executing the following section of a configuration file:

```yaml
python:
  initialize:
    build_file: /path/to/build.py
    required_files:
      - path: file1.txt
      - path: file999.txt
        alias: file2.txt
  jobs:
    - name: job_1
      file: execute_job.py
      required_files:
        - path: file3.txt
    - name: job_2
      file: execute_job.py
      required_files: []
```

When executing the compilation, the folder structure would look like this:

```
.
├── build.py
├── distribute_save
├── initial_files
│   ├── file1.txt
│   └── file2.txt
└── input
    ├── file1.txt
    ├── file2.txt
```

In other words: when building you only have access to the files from the `required_files` section in `initialize`. Another thing 
to note is that even though you have specified the path to the `file999.txt` file on your local computer, the file has *actually* 
been named `file2.txt` on the node. This is an additional feature to help your job execution scripts work uniform file names; you
dont actually need to need to keep a bunch of solver inputs named `solver_input.json` in separate folders to prevent name collision.
You can instead have several inputs `solver_input_1.json`, `solver_input_2.json`, `solver_input_3.json` on your local machine and
then set the `alias` filed to `solver_input.json` so that you run script can simply read the file at `./input/solver_input.json`!

Lets say your python build script (which has been renamed to `build.py` by `distribute` for uniformity) clones the STREAmS solver 
repository and compiled the project. Then, when executing `job_1` your folder structure would look something like this:

```
.
├── job.py
├── distribute_save
├── initial_files
│   ├── file1.txt
│   └── file2.txt
├── input
│   ├── file1.txt
│   ├── file2.txt
│   └── file3.txt
└── STREAmS
    ├── README.md
    └── src
        └── main.f90
```

Now, the folder structure is *exactly* as you have left it, plus the addition of a new `file3.txt` that you specified in your `required_files` 
section under `jobs`. Since `job_2` does not specify any additional `required_files`, the directory structure when running the python
script would look like this:

```
.
├── job.py
├── distribute_save
├── initial_files
│   ├── file1.txt
│   └── file2.txt
├── input
│   ├── file1.txt
│   ├── file2.txt
└── STREAmS
    ├── README.md
    └── src
        └── main.f90
```

In general, the presence of `./initial_files` is an implementation detail. The files in this section are *not* refreshed
between job executions. You should not rely on the existance of this folder - or modify any of the contents of it. The 
contents of the folder are copied to `./input` with every new job; use those files instead.
