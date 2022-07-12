# Available Capabilities

Current capabilities of nodes in our lab are tracked as an 
[example file](https://github.com/Fluid-Dynamics-Group/distribute/blob/master/static/example-nodes.yaml) 
in the repository. There are a few things to take away from this file:

## Capabilities for Apptainer jobs

The only required capability for an apptainer job is `apptainer`.
All dependencies and requirements can be handled by you in the apptainer definition file.

## Excluding Certain Machines from Executing Your Job

While any machine can run your jobs if they match the capabilities, sometimes you wish to avoid a machine 
if you know that someone will be running cases locally (not through the distributed system) and will simply
`distribute pause` your jobs - delaying the finish for your batch. 
To account for this possibility, you can add a capability `lab1` to only run the job on the `lab1` machine, `lab2` to only
run on `lab2`, etc. If you simply dont want to run on `lab3`, then you can specify `lab1-2`. Likewise, you can skip `lab1` with
`lab2-3`.
