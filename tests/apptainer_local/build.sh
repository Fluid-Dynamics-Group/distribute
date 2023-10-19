#! /usr/bin/bash

# this script should be executed in its directory!
# warning: this will create a folder at ~/apptainer/ and use it as a temporary directory
# for building the container. The reasoning for this is detailed in the apptainer
# documentation, but it it boils down to `/tmp` on linux being backed by an in-memory filesystem
# that apptainer will often exceed: https://apptainer.org/docs/user/main/build_env.html#temporary-folders
#
# this folder is safe to remove once the build operation is complete

OUT_SIF="apptainer_local.sif"
rm $OUT_SIF

echo "building common container"
mkdir -p ~/apptainer

# apptainer takes a TON of space in ~/tmp
# however, linux often mapts ~/tmp to memory so we can actually
# run out of memory when building containers often. This command 
# simply remaps the temporary build directory to a disk location

time APPTAINER_TMPDIR=~/apptainer sudo -E apptainer build $OUT_SIF ./apptainer_local.apt && \
	du -sh $OUT_SIF
