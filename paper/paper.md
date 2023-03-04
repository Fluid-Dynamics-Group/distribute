---
title: 'distribute: Easy to use Resource Manager for Distributed Computing without Assumptions'
tags:
  - Rust
  - Python
  - computing
  - gpu
  - linux
authors:
  - name: Brooks Karlik
    orcid: 0000-0002-1431-5199
    equal-contrib: true
    corresponding: true
    affiliation: 1
  - name: Aditya Nair
    equal-contrib: false
    orcid: 0000-0002-8979-8420
    affiliation: 1
affiliations:
 - name: University of Nevada, Reno
   index: 1
date: 3 March 2023
bibliography: paper.bib

---

# Summary

Distributed computing is an essential element for advancing computational sciences. Workload managers 
used by computing clusters help to meet the need for running large-scale simulations and efficiently 
sharing computational resources among multiple users [@reuther2016scheduler]. In this context, we 
introduce a tool designed to allocate computational resources effectively for small clusters of 
computers commonly found in research groups lacking specialized hardware such as computing fabric 
or shared file-systems. Additionally, compute nodes within the cluster can also serve as standard 
workstations for researchers, and any ongoing job on a machine can be temporarily suspended to 
enable normal research activities to proceed.

# Statement of need

Many research groups face challenges with inefficient utilization or over-scheduling of their 
computational resources. While some computers work for days to complete a queue of simulations, 
others remain unused. Dividing simulations manually among computers is time-consuming and often 
ineffective due to varying hardware configurations. Additionally, multiple individuals running 
jobs on a single computer can slow down progress for everyone involved.

Existing workload managers such as SLURM [@SLURM] and TORQUE [@TORQUE]
are useful for cluster computing but require specialized hardware not typically available on 
standard workstations. Furthermore, these workload managers assume that nodes are solely 
dedicated to computing, which may not always be the case.

Our tool, *distribute*, eliminates the need for specialized hardware such as memory fabric 
or shared filesystems, making almost no assumptions about the architecture of the computers. 
It schedules multiple jobs concurrently across several computers, as shown in
\autoref{fig:computing-layout}, avoiding the simultaneous execution of a single job across 
multiple computers. While using *distribute*, normal workstation usage may slow down 
during simulations, but we offer a way to temporarily pause background jobs to allow for normal work to continue.

Running a multidimensional parameter space sweep may result in hundreds of jobs, and our tool provides a 
Python package to programmatically generate the configuration file. Furthermore, *distribute* can 
transpile its job execution configuration to the SLURM format, making it easy to run a batch of 
jobs previously executed on a *distribute* cluster of in-house machines on a larger cluster with hundreds of cores.

![
Left: traditional horizontally scaling compute framework. Right: proposed compute framework.
\label{fig:computing-layout}
](./node_layout.png)

# Acknowledgements

AGN acknowledges the support the National Science Foundation AI Institute in Dynamic systems 
(Award no: 2112085, PM: Dr. Shahab Shojaei-Zadeh). 

# References
