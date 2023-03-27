# Input Files

In order for your compute job to do meaningful work, you will likely save some files. But we know that 
apptainer image files are not mutable. The answer to this problem is binding volumes. A "volume" 
is container-language for a folder inside the container that actually corresponds to a folder on the 
host system. Since these special folders ("volumes") are actually part of the host computer's 
filesystem they can be written to without error. The process of mapping a folder in your container
to a folder on the host system is called "binding". 

With apptainer, the binding of volumes to a container happens at runtime. Since `distribute` wants you to have
access to a folder to save things to (in python: `./distribute_save`), as well as a folder to read the `required_files`
you specified (in python: `./distribute_save`). Apptainer makes these folders slightly easier to access by binding them
to the root directory: `/distribute_save` and `/input`. When running your apptainer on the compute node with 16
cores, the following command is used to ensure that these bindings happen:

```bash
apptainer run apptainer_file.sif --app distribute --bind \
	path/to/a/folder:/distribute_save:rw,\
	path/to/another/folder:/input:rw\
	16
```

Note that the binding arguments are simply a comma separated list in the format `folder_on_host:folder_in_container:rw`
where `rw` specifies that files in the folder are readable and writeable.
If your configuration file for apptainer looks like this:

```yaml
meta:
  batch_name: your_jobset_name
  namespace: example_namespace
  capabilities: []
apptainer:
  initialize:
    sif: execute_container.sif
    required_files:
      - path: file1.txt
      - path: file999.txt
        alias: file2.txt
    required_mounts: []
  jobs:
    - name: job_1
      required_files:
        - path: file3.txt
    - name: job_2
      required_files: []
```

When running `job_1`, the `/input` folder looks like this:

```
input
├── file1.txt
├── file2.txt
└── file3.txt
```

And when running `job_2`, the `/input` folder looks like this:

```
input
├── file1.txt
├── file2.txt
```
