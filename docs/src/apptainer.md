# Apptainer

## Configuration File

A default configuration file can be generated with :

```
distribute template apptainer
```

```yaml
---
meta:
  batch_name: your_jobset_name
  namespace: example_namespace
  matrix: ~
  capabilities:
    - apptainer
    - gpu
apptainer:
  initialize:
    sif: 
	  path: execute_container.sif
    required_files:
      - path: /file/always/present/1.txt
        alias: optional_alias.txt
      - path: /another/file/2.json
        alias: ~
      - path: /maybe/python/utils_file.py
        alias: ~
    required_mounts:
      - /path/inside/container/to/mountL
  jobs:
    - name: job_1
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

### `apptainer` section

the `apptainer` section has two subsections: `initialize` which handles the setup of the solver
and `jobs` which specifies a list of jobs that are part of this job batch.

### `initialize` section

The `sif` key contains a `path` to a `.sif` file that was constructed from `apptainer build`.
Information on building these `.sif` files is available [here](./apptainer-introduction.md).
Ensure that you `.sif` file also contains the correct `%apprun distribute` interface.

The `required_files` contains a list of files, each file with a `path` and `alias`.
The `path` is a path to a file on disk, and the `alias` is a filename as it will appear
in the `/input` directory of your `.sif` solver when executed. A more in-depth 
explanation of this behavior is available in the [input files section](./apptainer-input-files.md)

`required_mounts` provides a way to create mutable directories in your container from within a compute 
node. Ensure that this directory does not exist in this container since it will be created at runtime.
More information on binding volumes (and creating mutable directories) is found [here](./apptainer-mutable-filesystem.md).

### `jobs`

This field contains the list of jobs and the locations of their input files on disk.
For each job, there are two configuration keys:

`name`: the name of the job in this batch. Job names must be unique to the `batch_name` field from
the `meta` section: there cannot be two jobs with the same name in the same batch. 

`required_files`: a list of files, each containing a `path` key and optional `alias` key. These
files are identical in configuration to the `required_files` from the `initialize` field above.
Each file has `path` (a location on disk) and an optional `alias` field, or the name of the file 
as it will be represented in the `/input` directory when that job is run.

For example, if the jobs section of the config file contained

```
	jobs:
	- name: job_1
	  required_files:
		- path: job_configuration_file.json
		  alias: ~
		- path: job_configuration_file_with_alias.json
		  alias: input.json
```

then this batch would have one job named `job_1`. When this job is executed, the `/input` directory of
the apptainer image will contain a file `job_configuration_file.json` and `input.json` since the 
second file was aliased. Additionally, any files specified in the `required_files` of the `initialize`
section will also be present in the `/input` directory.

## Workflow Differences From Python

The largest difference you will encounter between the apptainer and python configurations is the way in
which they are executed. While each python job has its own file that it may use for execution, the apptainer
workflow simply relies on whatever occurs in `%apprun distribute` to read files from `/input` and execute the 
binary directly. Therefore, each job in the configuration file only operates on some additional input files 
and the `.sif` file never changes. This is slightly less flexible than the python configuration (which allows
for individual python files to run each job), but by storing your input files in some intermediate structure
(like json) this difficulty can be easily overcome.
