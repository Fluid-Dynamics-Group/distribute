# Apptainer Overview

Apptainer (previously named Singularity) is a container system often used for packaging HPC applications. For us,
apptainer is useful for distributing your compute jobs since you can specify the exact dependencies required
for running. If your container runs on your machine, it will run on the `distribute`d cluster!

As mentioned in the introduction, you **must ensure that your container does not write to any directories that are not bound by the host
system**. This will be discussed further below, but suffice it to say that writing to apptainer's immutable filesystem
will crash your compute job.

## Versus Docker

There is an official documentation page discussing the differences between docker
and apptainer [here](https://apptainer.org/user-docs/master/singularity_and_docker.html). There a few primary
benefits for using apptainer from an implementation standpoint in `distribute`:

1. Its easy to use GPU compute from apptainer
2. Apptainer compiles down to a single `.sif` file that can easily be sent to the `distribute` server and passed to compute nodes
3. Once your code has been packaged in apptainer, it is very easy to run it on HPC clusters

## Apptainer definition files

This documentation is not the place to discuss the intricacies of apptainer definition files. A better overview can be found in the
official [apptainer documentation](https://apptainer.org/user-docs/master/definition_files.html). If you are familiar with 
docker, you can get up and running pretty quickly. Below is a short example of what a definition file looks like:

```
Bootstrap: docker
From: ubuntu:22.04

%files from build
	# in here you copy files / directories from your host machine into the 
	# container so that they may be accessed and compiled. 
	# the sintax is:

	/path/to/host/file /path/to/container/file

%post
	# install any extra packages required here
	# possibly with apt, or maybe pip3

%apprun distribute
	# execute your solver here
	# this section is called from a compute node
```
One *important* note from this file: the `%apprun distribute` section is critical. On a node 
with 16 cores, your `distribute` section gets called like this:

```
apptainer run --app distribute 16
```

In reality, this call is actually slightly more complex (see below), but this command is illustrative of the point.
**You must ensure you pass the number of allowed cores down to whatever run script / solver you are using, or start an MPI**. 
For example, to pass the information to a python script:

```
%apprun distribute
	cd /
	python3 /run.py $1
```

We make sure to pass down the `16` we received with `$1` which corresponds to "the first argument that this bash script was 
called with". Similar to the python configuration, your python file is also responsible for parsing this value and running 
your solver with the appropriate number of cores. You can parse the `$1` value you pass to python using the `sys.argv` value 
in your script:

```python
import sys
allowed_processors = sys.argv[1]
allowed_processors_int = int(allowed_processors)
assert(allowed_processors_int, 16)
```

**You must ensure that you use all (or as many) available cores on the machine as possible**! For the most part,
you **do not want to run single threaded processes on the distributed computing network - they will not go faster**.

If you are building an apptainer image based on nvidia HPC resources, your header would look something like this 
([nvidia documentation](https://catalog.ngc.nvidia.com/orgs/nvidia/containers/nvhpc)):

```
Bootstrap: docker
From: nvcr.io/nvidia/nvhpc:22.1-devel-cuda_multi-ubuntu20.05
```

## Building Apptainer Images

Compiling an apptainer definition file to a `.sif` file to run on the `distribute` compute is 
relatively simple (on linux). Run something like this:

```
mkdir ~/apptainer
APPTAINER_TMPDIR="~/apptainer" sudo -E apptainer build your-output-file.sif build.apptainer
```

where `your-output-file.sif` is the desired name of the `.sif` file that apptainer will spit out, and `build.apptainer` is the 
definition file you have built. The `APPTAINER_TMPDIR="~/apptainer"` portion of the command sets the `APPTAINER_TMPDIR` environment
variable to a location on disk (`~/apptainer`) because apptainer can often require more memory to compile the `sif` file
than what is available on your computer (yes, more than your 64 GB). Since `apptainer build` requires root privileges, it must be run with `sudo`. The additional
`-E` passed to `sudo` copies the environment variables from the host shell (which is needed to read `APPTAINER_TMPDIR`)
