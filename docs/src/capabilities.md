# Capabilities

Since capabilities are decided by the administrator for your `distribute` cluster, 
capabilities are cluster-specific.

One recommendation made to administrators, though, is that the capabilities `apptainer` and `gpu` are 
included to denote access to an `apptainer` binary on the system and the system containing a NVIDIA gpu, respectively.

As long as these capabilities are specified, users can bundle their own dependencies in an `apptainer` image
and execute easily. If your group intends to make heavy use of `python` configurations across
multiple computers, you may run into robustness issues: these workflows are frail, easy to break, and hard to maintain.
