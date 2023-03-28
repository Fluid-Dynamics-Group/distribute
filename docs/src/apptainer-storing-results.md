# Storing Results

Simiar to how input files are placed in the `/input` directory, output files for apptainer
jobs are placed in `/distribute_save`.

Any file placed in `/distribute_save` when your `apptainer` image ends is automatically transported
back to the head node and can be downloaded with the `distribute pull` [command](./commands.md#pull)

All directories will be wiped between simulation runs, including `/input` and `/distribute_save`. 
You may not rely on any files from a previous job to be present in any directory at the start of execution: only
files specified in the corresponing `required_files` fields will be supplied.
