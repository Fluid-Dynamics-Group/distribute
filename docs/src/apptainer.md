# Apptainer

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

1. Its easy to use GPU compute from singularity
2. Apptainer compiles down to a single `.sif` file that can easily be sent to the `distribute` server and passed to compute nodes
3. Once your code has been packaged in singularity, it is very easy to run it on paid HPC clusters

## Apptainer definition files

This documentation is not the place to discuss the intricacies of singularity / apptainer. As a user, we have tried to make
it as easy as possible to build an image that can run on `distribute`. 
The [apptainer-common](https://github.com/Fluid-Dynamics-Group/apptainer-common) was purpose built to give you a good
starting place with compilers and runtimes (including fortran, C++, openfoam, python3). Your definition file
needs to look something like this:

```
Bootstrap: library
From: library://vanillabrooks/default/fluid-dynamics-common

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

A (simplified) example of a definition file I have used is this:

```
Bootstrap: library
From: library://vanillabrooks/default/fluid-dynamics-common

%files
	# copy over my files
	/home/brooks/github/hit3d/ /hit3d
	/home/brooks/github/hit3d-utils/ /hit3d-utils
	/home/brooks/github/vtk/ /vtk
	/home/brooks/github/vtk-analysis/ /vtk-analysis
	/home/brooks/github/fourier/ /fourier
	/home/brooks/github/ndarray-gradient/ /ndarray-gradient
	/home/brooks/github/matrix-notify/ /matrix-notify
	/home/brooks/github/distribute/ /distribute

%environment
	CARGO_TARGET_DIR="/target"

%post
	# add cargo to the environment
	export PATH="$PATH":"$HOME/.cargo/bin"

	cd /hit3d-utils
	cargo install --path .
	ls -al /hit3d-utils

	cd /hit3d/src
	make

	cd /vtk-analysis
	cargo install --path .

	# move the binaries we just installed to the root
	mv $HOME/.cargo/bin/hit3d-utils /hit3d/src
	mv $HOME/.cargo/bin/vtk-analysis /hit3d/src

	#
	# remove directories that just take up space
	#
	rm -rf /hit3d/.git
	rm -rf /hit3d/target/
	rm -rf /hit3d/src/output/
	rm -rf /hit3d-utils/.git
	rm -rf /hit3d-utils/target/

	#
	# simplify some directories
	#
	mv /hit3d/src/hit3d.x /hit3d.x

	# copy the binaries to the root
	mv /hit3d/src/vtk-analysis /vtk-analysis-exe
	mv /hit3d/src/hit3d-utils /hit3d-utils-exe

	mv /hit3d-utils/plots /plots

	mv /hit3d-utils/generic_run.py /run.py

%apprun distribute
	cd /
	python3 /run.py $1
```

I want to emphasize one specific thing from this file: the `%apprun distribute` section is very important. On a node 
with 16 cores, your `distribute` section gets called like this:

```
singularity run --app distribute 16
```

**You must ensure you pass the number of allowed cores down to whatever run script you are using**. In our example:

```
%apprun distribute
	cd /
	python3 /run.py $1
```

We make sure to pass down the `16` we received with `$1` which corresponds to "the first argument that this bash script was 
called with". Similar to the python configuration, your python file is also responsible for parsing this value and running 
your solver with the appropriate number of cores. You can parse the `$1` value you pass to python using the `sys.argv` value 
in your script:

```
import sys
allowed_processors = sys.argv[1]
allowed_processors_int = int(allowed_processors)
assert(allowed_processors_int, 16)
```

**You must ensure that you use all available cores on the machine**. If your code can only use a reduced number
of cores, make sure you specify this in your `capabilities` section! **Do not run single threaded
processes on the distributed computing network - they will not go faster**.

## Binding Volumes (Mutable Filesystems)
