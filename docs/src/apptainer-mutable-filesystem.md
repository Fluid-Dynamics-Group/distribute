# Mutable Filesystem (Binding Volumes)

Now a natural question you may have is this: If volume bindings are specified at runtime - and not
within my apptainer definition file - how can I possibly get additional mutable folders? Am I stuck
with writing to `/input` and `/distribute_save`? The answer is no! You can tell `distribute` what folders
in your container you want to be mutable with the `required_mounts` key in the `initialize` section of 
your configuration. For example, in the hit3d solver (whose definition file is used as the example
above), the following folder structure at `/` would be present at runtime:

```
.
├── distribute_save
├── hit3d-utils-exe
├── hit3d.x
├── input
├── plots
│   ├── energy_helicity.py
│   ├── proposal_plots.py
│   └── viscous_dissapation.py
└── vtk-analysis-exe
```

However, `hit3d` *requires* a folder called `output` relative to itself. Since this folder is required,
we might be (naively) tempted to simply add a call to `mkdir /output` in  our `%post` section of the 
definition file. However, we would then be creating an *immutable* directory within the image. Instead,
we simply just need to add this path to our configuration file:

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
    required_mounts:
	  - /output				# <---- here
  jobs:
    - name: job_1
      required_files:
        - path: file3.txt
    - name: job_2
      required_files: []
```

By adding this line, your container will be invoked like this (on a 16 core machine):

```
apptainer run apptainer_file.sif --app distribute --bind \
	path/to/a/folder:/distribute_save:rw,\
	path/to/another/folder:/input:rw,\
	path/to/yet/another/folder/:/output:rw\
	16
```
