---
title: 'distribute: Easy to use Distributed Computing without Assumptions'
tags:
  - Rust
  - computing
  - gpu
  - linux
authors:
  - name: Brooks Karlik
    orcid: 0000-0000-0000-0000
    equal-contrib: true
	corresponding: true
    affiliation: 1
  - name: Aditya Nair
    orcid: 0000-0000-0000-0000
    affiliation: 1
affiliations:
 - name: University of Nevada, Reno
   index: 1
date: 30 January 2023
bibliography: paper.bib

# Optional fields if submitting to a AAS journal too, see this blog post:
# https://blog.joss.theoj.org/2018/12/a-new-collaboration-with-aas-publishing
# aas-doi: 10.3847/xxxxx <- update this with the DOI from AAS once you know it.
# aas-journal: Astrophysical Journal <- The name of the AAS journal.
---

# Summary

Distributed computing is foundational to the progress of computational sciences. Workload managers
employed by computing clusters
address the requirement for running simulations at scale as well as sharing a pool of computational
resources between multiple users. We present a tool to efficiently allocate computational resources 
for small clusters of computers found in many research groups without specialized hardware 
(memory fabric, shared filesystems, etc). Compute nodes in the cluster may also function as standard
workstations for researchers; any currently running job on a machine may be suspended temporarily 
for normal research to be carried out.

# Statement of need

Computational resources of many research groups are either underutilized or over-scheduled. 
While one computer may work several days to get through a queue of simulations, other computers
that are available to the group sit unused. Manually dividing up simulations to run on different computers
is time consuming, and with heterogeneous hardware configurations, usually inefficient. Moreover, 
different group members may run jobs on a given computer at the same time, slowing the progress of 
both.

The use of existing workload managers ([@SLURM], [@TORQUE], etc) address cluster computing well, but require
specialized hardware that standard workstations are not equipped with. Moreover, these workload managers
assume that the nodes they run on are exclusively used for compute and allocate resources accordingly.

distribute makes almost no assumptions about the architecture of your computers: no memory fabric or shared
filesystems are required to operate it. distribute does not attempt to run a single job across many computers
simultaneously, but instead solves computing problems by scheduling several jobs on several computers
concurrently. Since distribute is built to utilize existing hardware, the normal use of the workstations 
may be slowed while the simulations are executed. To alleviate this burden, distribute provides a way to pause
the execution of a background job on a system for normal work to continue.

Sweeps over combination of a dozen simulation parameters may lead to hundreds of jobs to run, so distribute
provides a python package to generate its configuration file programmatically.
distribute also provides a way to transpile its job execution configuration to a SLURM configuration format.
Therefore, a batch of jobs that was previously run on a distrbute cluster of in-house machines may be easily
translated to run on a much larger cluster and execute on hundreds of cores concurrently.

# Citations

Citations to entries in paper.bib should be in
[rMarkdown](http://rmarkdown.rstudio.com/authoring_bibliographies_and_citations.html)
format.

If you want to cite a software repository URL (e.g. something on GitHub without a preferred
citation) then you can do it with the example BibTeX entry below for @fidgit.

For a quick reference, the following citation commands can be used:
- `@author:2001`  ->  "Author et al. (2001)"
- `[@author:2001]` -> "(Author et al., 2001)"
- `[@author1:2001; @author2:2001]` -> "(Author1 et al., 2001; Author2 et al., 2002)"

# Figures

Figures can be included like this:
![Caption for example figure.\label{fig:example}](figure.png)
and referenced from text using \autoref{fig:example}.

Figure sizes can be customized by adding an optional second parameter:
![Caption for example figure.](figure.png){ width=20% }

# Acknowledgements

We acknowledge contributions from Brigitta Sipocz, Syrtis Major, and Semyeong
Oh, and support from Kathryn Johnston during the genesis of this project.

# References
