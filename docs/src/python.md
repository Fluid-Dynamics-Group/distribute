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

## What You Are Provided

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

## Saving results of your compute jobs

Archiving jobs to the head node is *super* easy. All you have to do is ensure that your execution script moves all files
you wish to save to the `./distribute_save` folder before exiting. `distribute` will automatically read all the files
in `./distribute_save` and save them to the corresponding job folder on the head node permenantly. `distribute` will 
also clear out the `./distribute_save` folder for you between jobs so that you dont end up with duplicate files.


## Build Scripts

The build script is specified in the `initialize` section under the `build_file` key. The build script is simply responsible
for cloning relevant git repositories and compiling any scripts in the project. Since private repositories require
a github SSH key, a read-only ssh key is provided on the system so that you can clone any private `fluid-dynamics-group`
repo. An example build script that I have personally used for working with `hit3d` looks like this:

```python
import subprocess
import os
import sys
import shutil
# hit3d_helpers is a python script that I have specified in 
# my `required_files` section of `initialize`
from initial_files import hit3d_helpers
import traceback

HIT3D = "https://github.com/Fluid-Dynamics-Group/hit3d.git"
HIT3D_UTILS = "https://github.com/Fluid-Dynamics-Group/hit3d-utils.git"
VTK = "https://github.com/Fluid-Dynamics-Group/vtk.git"
VTK_ANALYSIS = "https://github.com/Fluid-Dynamics-Group/vtk-analysis.git"
FOURIER = "https://github.com/Fluid-Dynamics-Group/fourier-analysis.git"
GRADIENT = "https://github.com/Fluid-Dynamics-Group/ndarray-gradient.git"
DIST = "https://github.com/Fluid-Dynamics-Group/distribute.git"
NOTIFY = "https://github.com/Fluid-Dynamics-Group/matrix-notify.git"

# executes a command as if you were typing it in a terminal
def run_shell_command(command):
    print(f"running {command}")
    output = subprocess.run(command,shell=True, check=True)
    if not output.stdout is None:
        print(output.stdout)

# construct a `git clone` string to run as a shell command
def make_clone_url(ssh_url, branch=None):
    if branch is not None:
        return f"git clone -b {branch} {ssh_url} --depth 1"
    else:
        return f"git clone {ssh_url} --depth 1"

def main():
    build = hit3d_helpers.Build.load_json("./initial_files")

    print("input files:")
    run_shell_command(make_clone_url(HIT3D, build.hit3d_branch))
    run_shell_command(make_clone_url(HIT3D_UTILS, build.hit3d_utils_branch))
    run_shell_command(make_clone_url(VTK))
    run_shell_command(make_clone_url(VTK_ANALYSIS))
    run_shell_command(make_clone_url(FOURIER))
    run_shell_command(make_clone_url(GRADIENT))
    run_shell_command(make_clone_url(DIST, "cancel-tasks"))
    run_shell_command(make_clone_url(NOTIFY))
    
    # move the directory for book keeping purposes
    shutil.move("fourier-analysis", "fourier")

    # build hit3d
    os.chdir("hit3d/src")
    run_shell_command("make")
    os.chdir("../../")

    # build hit3d-utils
    os.chdir("hit3d-utils")
    run_shell_command("cargo install --path .")
    os.chdir("../")

    # build vtk-analysis
    os.chdir("vtk-analysis")
    run_shell_command("cargo install --path .")
    os.chdir("../")

    # all the other projects cloned are dependencies of the built projects
    # they don't need to be explicitly built themselves

if __name__ == "__main__":
    main()
```

note that `os.chdir` is the equivalent of the GNU coreutils `cd` command: it simply changes the current working 
directory.

## Job Execution Scripts

Execution scripts are specified in the `file` key of a list item a job `name` in `jobs`. Execution scripts
can do a lot of things. I have found it productive to write a single `generic_run.py` script that
reads a configuration file from `./input/input.json` is spefied under my `required_files` for the job)
and then run the sovler from there. 

One import thing about execution scripts is that they are run with a command line argument specifying
how many cores you are allowed to use. If you hardcode the number of cores you use you will either
oversaturate the processor (therefore slowing down the overall execution speed), or undersaturate 
the resources available on the machine. Your script will be "executed" as if it was a command line 
program. If the computer had 16 cores available, this would be the command:

```bash
python3 ./job.py 16
```

you can parse this value using the `sys.argv` value in your script:

```python
import sys
allowed_processors = sys.argv[1]
allowed_processors_int = int(allowed_processors)
assert(allowed_processors_int, 16)
```

**You must ensure that you use all available cores on the machine**. If your code can only use a reduced number
of cores, make sure you specify this in your `capabilities` section! **Do not run single threaded
processes on the distributed computing network - they will not go faster**.

A full working example of a run script that I use is this:

```python
import os
import sys
import json
from input import hit3d_helpers
import shutil
import traceback

IC_SPEC_NAME = "initial_condition_espec.pkg"
IC_WRK_NAME = "initial_condition_wrk.pkg"

def load_json():
    path = "./input/input.json"

    with open(path, 'r') as f:
        data = json.load(f)
        print(data)
        return data

# copies some initial condition files from ./input 
# to the ./hit3d/src directory so they can be used 
# by the solver
def move_wrk_files(is_root):
	outfile = "hit3d/src/"
	infile = "input/"

    shutil.copy(infile + IC_SPEC_NAME, outfile + IC_SPEC_NAME)
    shutil.copy(infile + IC_WRK_NAME, outfile + IC_WRK_NAME)


# copy the ./input/input.json file to the output directory
# so that we can see it later when we download the data for viewing
def copy_input_json(is_root):
	outfile = "distribute_save/"
	infile = "input/"

    shutil.copy(infile  + "input.json", outfile + "input.json")

if __name__ == "__main__":
    try:
        data = load_json();

        # get the number of cores that we are allowed to use from the command line
        nprocs = int(sys.argv[1])

        print(f"running with nprocs = ", nprocs)

        # parse the json data into parameters to run the solver with
        skip_diffusion = data["skip_diffusion"]
        size = data["size"]
        dt = data["dt"]
        steps = data["steps"]
        restarts = data["restarts"]
        reynolds_number = data["reynolds_number"]
        path = data["path"]
        load_initial_data = data["load_initial_data"]
        export_vtk = data["export_vtk"]
        epsilon1 = data["epsilon1"]
        epsilon2 = data["epsilon2"]
        restart_time = data["restart_time"]
        skip_steps = data["skip_steps"]
        scalar_type = data["scalar_type"]
        validate_viscous_compensation = data["validate_viscous_compensation"]
        viscous_compensation = data["viscous_compensation"]
        require_forcing = data["require_forcing"]
        io_steps = data["io_steps"]
        export_divergence = data["export_divergence"]
        
		# if we need initial condition data then we copy it into ./hit3d/src/
        if not load_initial_data == 1:
            move_wrk_files()

        root = os.getcwd()

        # open hit3d folder
        openhit3d(is_root)

		# run the solver using the `hit3d_helpers` file that we have
		# ensured is present from `required_files`
        hit3d_helpers.RunCase(
            skip_diffusion,
            size,
            dt,
            steps, 
            restarts, 
            reynolds_number,
            path,
            load_initial_data, 
            nprocs, 
            export_vtk, 
            epsilon1,
            epsilon2, 
            restart_time,
            skip_steps,
            scalar_type,
            validate_viscous_compensation,
            viscous_compensation,
            require_forcing,
            io_steps,
            export_divergence
        ).run(0)

        # go back to main folder that we started in 
        os.chdir(root)

        copy_input_json(is_root)

        sys.exit(0)

    # this section will ensure that the exception and traceback 
	# is printed to the console (and therefore appears in stdout files saved
	# on the server
    except Exception as e:
        print("EXCEPTION occured:\n",e)
        print(e.__cause__)
        print(e.__context__)
        traceback.print_exc()
        sys.exit(1)
```


## Full Example

A simpler example of a python job has been compiled and verified 
[here](https://github.com/Fluid-Dynamics-Group/distribute/tree/cancel-tasks/examples/python).
