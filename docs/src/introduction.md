# distribute

`distribute` is a relatively simple command line utility for distributing compute jobs across the powerful
lab computers. In essence, `distribute` provides a simple way to automatically schedule dozens of jobs 
from different people across the small number of powerful computers in the lab. 

## Zero downtime computing

`distribute` relies on some simple programming interfaces. From a batch of jobs (1-100s), `distribute`
will automatically schedule each job in the batch to a compute node and archive the results on the 
head node. After archival, the results of a batch are easy to pull down to a personal computer
using a simple set of query rules and the original configuration file submitted to the cluster.
Excess data can be deleted from a personal computer and re-downloaded later at will.

`distribute` is also partially fault tolerant: if a compute node goes down while executing a job,
the job will be rescheduled to another compatible node. Once the original compute node is 
restored, jobs will automatically resume execution on the node.

## Heterogeneous Compute Clusters

`distribute` works on a simple "capability" based system to ensure that a batch of jobs 
is only scheduled across a group of compute nodes that are compatible. For instance, specifying that
a job requires large amounts of memory, a GPU, or a certain number of CPU cores.

## SLURM Compatibility

You can seamlessly transpile a configuration file for hundreds of `distribute` jobs to a SLURM-compatible format.
Therefore, you can schedule several jobs on a local `distribute` cluster and then rerun the jobs 
on a University cluster with a finer computational stencil or longer runtime seamlessly.

## Pausing Background Jobs

Since lab computers also function as day-to-day workstations for researchers, some additional
features are required to ensure that they are functional outside of running jobs. `distribute` solves this issue 
by allowing a user that is sitting at a computer to temporarily pause the currently executing job so that
they may perform some simple work. This allows users to still quickly iterate on ideas without waiting
hours for their jobs to reach the front of the queue. 
This behavior is incompatible with the philosophy of other workload managers such as SLURM.

## Matrix Notifications

If setup with [matrix](https://matrix.org/) API keys, `distribute` can send you messages on the completion of your 
jobs.

## Python API

We have thus far talked about all the cool things we can do with `distribute`, but none of this is free. As
a famous Italian engineer once said, "There is no such thing as free lunch." There are two complexities 
from a user's point of view:

1. Generating Configuration Files
2. Packaging software in a compatible way for the cluster

To alleviate the first point, `distribute` provides a short but well documented python package 
to generate configuration files (short files can also be written by hand). 
This makes it easy to perform sweeps with hundreds of jobs over a large parameter space.
An example python configuration is below:

```yaml
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
    build_file: 
	  path: /path/to/build.py
  jobs:
    - name: job_1
      file: execute_job.py
    - name: job_2
      file: execute_job_2.py
```
