# Command Basics

There are a few commands that you will need to know to effectively work with `distribute`. Don't worry,
they are not too complex. The full list of commands and their specific parameters can be found by running

```bash
$ distribute
```

at the time of writing, this yields:

```
distribute 0.9.4
A utility for scheduling jobs on a cluster

USAGE:
    distribute [FLAGS] <SUBCOMMAND>

FLAGS:
    -h, --help         Prints help information
        --save-log
        --show-logs
    -V, --version      Prints version information

SUBCOMMANDS:
    add              add a job set to the queue
    client           start this workstation as a node and prepare it for a server connection
    help             Prints this message or the help of the given subcommand(s)
    kill             terminate any running jobs of a given batch name and remove the batch from the queue
    node-status      check the status of all the nodes
    pause            pause all currently running processes on this node for a specified amount of time
    pull             Pull files from the server to your machine
    run              run a apptainer configuration file locally (without sending it off to a server)
    server           start serving jobs out to nodes using the provied configuration file
    server-status    check the status of all the nodes
    template         generate a template file to fill for executing with `distribute add`
```

## add

`distribute add` is how you can add jobs to the server queue. There are two main things needed to operate this command: 
a configuration file and the IP of the main server node. If you do not specify the name of a configuration
file, it will default to `distribute-jobs.yaml`. This command can be run (for most cases) as such:

```bash
distribute add --ip <server ip address here> my-distribute-jobs-file.yaml
```

or, using defaults:

```bash
distribute add --ip <server ip address here>
```

If there exists no node that matches all of your required capabilities, the job will not be run. There also exists a `--dry` flag
if you want to check that your configuration file syntax is correct, and a `--show-caps` flag to print the capabilities 
of each node.


## template 

`distribute template` is a simple way to create a `distribute-jobs.yaml` file that either runs with `python` or `apptainer`s. The specifics
of each configuration file will be discussed later.

```bash
distribute template python
```

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

```bash
distribute template apptainer
```

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
apptainer:
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

If you use a compute node as a work station, `distribute pause` will pause all locally running jobs so that you 
can use the workstation normally. It takes a simple argument as an upper bound on how long the tasks can be paused. The maximum amount of time that 
a job can be paused is four hours (`4h`), but if this is not enough you can simply rerun the command. This 
upper bound is just present to remove any chance of you accidentally leaving the jobs paused for an extended
period of time.

If you decide that you no longer need the tasks paused, you can simply `Ctrl-C` to quit the hanging command
and all processes will be automatically resumed. **Do not close your terminal** before the pausing finishes or
you have canceled it with `Ctrl-C` as the job on your machine will never resume.

some examples of this command:

```bash
sudo distribute pause --duration 4h
```

```bash
sudo distribute pause --duration 1h30m10s
```

```bash
sudo distribute pause --duration 60s
```

## server-status

`distribute status` prints out all the running jobs at the head node. It will show you all the job batches
that are currently running, as well as the number of jobs in that set currently running and the 
names of the jobs that have not been run yet. You can use this command to fetch the required parameters
to execute the `kill` command if needed.

```bash
distribute server-status --ip <server ip here>
```

If there is no output then there are no jobs currently in the queue or executing on nodes.

**TODO** An example output here

```
260sec
        :jobs running now: 1
10sec_positive
        -unforced_viscous_decay
        -unforced_inviscid_decay
        -viscous_forcing_no_compensation_eh_first
        -viscous_forcing_no_compensation_eh_second
        -viscous_forcing_no_compensation_eh_both
        :jobs running now: 0
```

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

```yaml
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

```bash
distribute pull distribute-jobs.yaml --ip <server ip here> \
	exclude \
		--exclude "vtk"
```

Or, if you wanted to exclude all of the case3 files and all vtk files:

```bash
distribute pull distribute-jobs.yaml --ip <server ip here> \
	exclude \
		--exclude "vtk" \
		--exclude "case3"
```

Maybe you only want to pull case1 files:

```bash
distribute pull distribute-jobs.yaml --ip <server ip here> \
	include \
		--include "case1"
```


## run

`distribute run` will run an apptainer job locally. It is usefull for debugging apptainer jobs
since the exact commands that are passed to the container are not always intuitive. 

```
distribute run --help
```

```
distribute-run 0.6.0
run a apptainer configuration file locally (without sending it off to a server)

USAGE:
    distribute run [FLAGS] [OPTIONS] [job-file]

FLAGS:
        --clean-save    allow the save_dir to exist, but remove all the contents of it before executing the code
    -h, --help          Prints help information
    -V, --version       Prints version information

OPTIONS:
    -s, --save-dir <save-dir>    the directory where all the work will be performed [default: ./distribute-run]

ARGS:
    <job-file>    location of your configuration file [default: distribute-jobs.yaml]
```

An example is provided in the apptainer jobs section.
