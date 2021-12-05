# Command Basics

There are a few commands that you will need to know to effectively work with `distribute`. Don't worry,
they are not too complex. The full list of commands and their specific parameters can be found by running

```
$ distribute
```

at the time of writing, this yields:

```
distribute 0.6.0
A utility for scheduling jobs on a cluster

USAGE:
    distribute <SUBCOMMAND>

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information

SUBCOMMANDS:
    add         add a job set to the queue
    client      start this workstation as a node and prepare it for a server connection
    help        Prints this message or the help of the given subcommand(s)
    kill        terminate any running jobs of a given batch name and remove the batch from the queue
    pause       pause all currently running processes on this node for a specified amount of time
    pull        Pull files from the server to your machine
    server      start serving jobs out to nodes using the provied configuration file
    status      check the status of all the nodes
    template    generate a template file to fill for executing with `distribute add`
```



## add

`distribute add` is how you can add jobs to the server queue. There are two main things needed to operate this command: 
a configuration file and the IP of the main server node. If you do not specify the name of a configuration
file, it will default to `distribute-jobs.yaml`. This command can be run (for most cases) as such:

```
distribute add --ip <server ip address here> my-distribute-jobs-file.yaml
```

or, using defaults:

```
distribute add --ip <server ip address here>
```

If there exists no node that matches all of your required capabilities, the job will not be run. There also exists a `--dry` flag
if you want to check that your configuration file syntax is correct, and a `--show-caps` flag to print the capabilities 
of each node.


## template 

`distribute template` is a simple way to create a `distribute-jobs.yaml` file that either runs with `python` or `apptainer`s. The specifics
of each configuration file will be discussed later.

```
distribute template python
```

```
---
meta:
  batch_name: your_jobset_name
  namespace: example_namespace
  matrix: ~
  capabilities:
    - gfortran
    - python3
    - singularity
python:
  initialize:
    build_file: /path/to/build.py
    required_files:
      - path: /file/always/present/1.txt
        alias: optional_alias.txt
      - path: /another/file/2.json
        alias: ~
      - path: /maybe/python/utils_file.py
        alias: ~
  jobs:
    - name: job_1
      file: execute_job.py
      required_files:
        - path: job_configuration_file.json
          alias: ~
        - path: job_configuration_file_with_alias.json
          alias: input.json
```

and

```
distribute template singularity
```

```
---
meta:
  batch_name: your_jobset_name
  namespace: example_namespace
  matrix: ~
  capabilities:
    - gfortran
    - python3
    - singularity
singularity:
  initialize:
    sif: execute_container.sif
    required_files:
      - path: /file/always/present/1.txt
        alias: optional_alias.txt
      - path: /another/file/2.json
        alias: ~
      - path: /maybe/python/utils_file.py
        alias: ~
    required_mounts:
      - /path/inside/container/to/mount
  jobs:
    - name: job_1
      required_files:
        - path: job_configuration_file.json
          alias: ~
        - path: job_configuration_file_with_alias.json
          alias: input.json
```

## pause

`distribute pause` will pause all locally running jobs so that you can use the workstation normally. It takes
a simple argument as an upper bound on how long the tasks can be paused. The maximum amount of time that 
a job can be paused is four hours (`4h`), but if this is not enough you can simply rerun the command. This 
upper bound is just present to remove any chance of you accidentally leaving the jobs paused for an extended
period of time.

If you decide that you no longer need the tasks paused, you can simply `Ctrl-C` to quit the hanging command
and all processes will be automatically resumed.

some examples of this command:

```
distribute pause 4h
```

```
distribute pause 1h30m10s
```

```
distribute pause 60s
```

## status

`distribute status` prints out all the running jobs at the head node. It will show you all the job batches
that are currently running, as well as the number of jobs in that set currently running and the 
names of the jobs that have not been run yet. You can use this command to fetch the required parameters
to execute the `kill` command if needed.

```
distribute status --ip <server ip here>
```

**TODO** An example output here

## pull

`distribute pull` takes a `distribute-jobs.yaml` config file and pulls all the files associated with that batch
to a specified `--save-dir` (default is the current directory). This is really convenient because the only thing
you need to fetch your files is the original file you used to compute the results in the first place!

Since you often dont want to pull *all the files* - which might include tens or hundreds of gigabytes of flowfield
files - this command also accepts `include` or `exclude` filters, which consist of a list of regular expressions
to apply to the file path.  If using a `include` query, any file matching one of the regexs will be pulled to 
your machine. If using a `exclude` query, any file matching a regex will *not* be pulled to your computer. 

The full documentation on regular expressions is found [here](https://docs.rs/regex/latest/regex/), but luckily 
most character strings are valid regular exprssions (barring characters like `+`, `-`, `(`, `)`). Lets say your
`meta` section of the config file looks like this:

```
---
meta:
  batch_name: incompressible_5second_cases
  namespace: brooks_openfoam_cases
  capabilities: []
```

and your directory tree looks something like this

```
├── incompressible_5second_cases
    ├── case1
    │   ├── flowfield.vtk
    │   └── statistics.csv
    ├── case2
    │   ├── flowfield.vtk
    │   └── statistics.csv
    └── case3
        ├── flowfield.vtk
        └── statistics.csv
```

If you wanted to exclude any file with a `vtk` extension, you could

```
distribute pull distribute-jobs.yaml --ip <server ip here> \
	exclude \
		--exclude "vtk"
```

Or, if you wanted to exclude all of the case3 files and all vtk files:

```
distribute pull distribute-jobs.yaml --ip <server ip here> \
	exclude \
		--exclude "vtk" \
		--exclude "case3"
```

Maybe you only want to pull case1 files:

```
distribute pull distribute-jobs.yaml --ip <server ip here> \
	include \
		--include "case1"
```