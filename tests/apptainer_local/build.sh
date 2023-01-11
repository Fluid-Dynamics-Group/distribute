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
