# Packaging Solvers

In order to package a given solver into apptainer there are three main tasks:

1. Generate a apptainer definition file to compile code
2. For large numbers of jobs:
	* a python script to generate input files to the solver and generate `distribute` configuration files
3. Determine what directories in your container should be mutable (often none), include those paths in your configuration file

![](./figs/apptainer_config_flowchart.png)
