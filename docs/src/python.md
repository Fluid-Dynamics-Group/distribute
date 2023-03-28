# Python

Python configuration file templates can be generated as follows:

```
distribute template python
```

At the time of writing, it outputs something like this:

```yaml
---
meta:
  batch_name: your_jobset_name
  namespace: example_namespace
  matrix: ~
  capabilities:
    - gfortran
    - python3
    - apptainer
python:
  initialize:
    build_file: 
	  path: /path/to/build.py
    required_files:
      - path: /file/always/present/1.txt
        alias: optional_alias.txt
      - path: /another/file/2.json
        alias: ~
      - path: /maybe/python/utils_file.py
        alias: ~
  jobs:
    - name: job_1
      file: 
	    path: execute_job.py
      required_files:
        - path: job_configuration_file.json
          alias: ~
        - path: job_configuration_file_with_alias.json
          alias: input.json
```

### `meta` section

The `meta` section is identical to the `meta` section of python. 

The `namespace` field is a string that describes all of the `batch_name`s that 
are a part of this `namespace`.

The `batch_name` is an a simple string to identify what jobs are in this batch.

If your `distribute` cluster has been configured by your adminstrator for matrix support, 
you can specify the `matrix` field to be your matrix username in the form of `@your-user:matrix.org`
and you will receive a message when all of the jobs in this job batch finish.

The `capabilities` section is a list of dependencies that are required to be present on the compute node.
Since you are responsible for bundling all your dependencies in your apptainer image, the only capabilities
requires here are `apptainer`. If you require a GPU on the compute node, you can also specify the `gpu`
capability.

On the server, each `batch_name` must be unique to each `namespace`. This means the results of submitting
two batches of batch name `name_xyz` with identical namespaces is undefined. This is because on
the server each batch name is a subdirectory of the namespace. For example, if we have run
two job batches `job_batch_01` and `job_batch_02` under the namespace `example_namespace`,
the directory structure might look like 

```
example_namespace/
├── job_batch_01
│   ├── job_1
│   │   └── stdout.txt
│   ├── job_2
│   │   └── stdout.txt
│   └── job_3
│       └── stdout.txt
└── job_batch_02
    ├── job_1
    │   └── stdout.txt
    ├── job_2
    │   └── stdout.txt
    └── job_3
        └── stdout.txt
```

For example, if I am running many aeroacoustic studies with OpenFoam, a namespace might be 
`austin_openfoam_aeroacoustic`, and then I run batches with incremented names,
`supersonic_aeroacoustic_00X`, `subsonic_aeroacoustic_00X`, `transsonic_aeroacoustic_00X`. My `meta`
section for one of these job batches would look like:

```
meta:
  batch_name: subsonic_aeroacoustic_004
  namespace: austin_openfoam_aeroacoustic
  matrix: @my-username:matrix.org
  capabilities:
    - apptainer
    - gpu
```

### `python` section

the `python` section has two subsections: `initialize` which handles compiling and setting
up all code for execution, and `jobs` which specifies a list of jobs that are part of this job batch.

### `initialize` section

The `build_file` key contains a `path` to a python file.
This python file is responsible for compiling and setting up an environment on the system
to execute all of the jobs in the job batch.

The `required_files` contains a list of files, each file with a `path` and `alias`.
The `path` is a path to a file on disk, and the `alias` is a filename as it will appear
in the `/input` directory when compiling all related tasks. 
An in depth explanation of input files is detailed in the [input files section](./python-input-files.md)

### `jobs`

This field contains the list of jobs, the file used to execute the job,
and the location to other input files on disk.
For each job, there are three configuration keys:

`name`: the name of the job in this batch. Job names must be unique to the `batch_name` field from
the `meta` section: there cannot be two jobs with the same name in the same batch. 

`file`: contains a `path` key to a python file on disk that will be used to operate on
any input files and execute any required code for this job.

`required_files`: a list of files, each containing a `path` key and optional `alias` key. These
files are identical in configuration to the `required_files` from the `initialize` field above.
Each file has `path` (a location on disk) and an optional `alias` field, or the name of the file 
as it will be represented in the `/input` directory when that job is run.

For example, if the jobs section of the config file contained

```
  jobs:
    - name: job_1
      file: 
	    path: execute_job.py
      required_files:
        - path: job_configuration_file.json
          alias: ~
        - path: job_configuration_file_with_alias.json
          alias: input.json
```

then this batch would have one job named `job_1`. When this job is executed, the `./input` directory of
the apptainer image will contain a file `job_configuration_file.json` and `input.json` since the 
second file was aliased. Additionally, any files specified in the `required_files` of the `initialize`
section will also be present in the `./input` directory.

At the start of the execution, your `execute_job.py` file will be called and it will operate on
the files in `./input` to produce results. Results are stored in the `./distribute_save` directory.
