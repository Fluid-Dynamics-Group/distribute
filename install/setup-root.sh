# download paraview 5.9.1 headless
wget --output-document paraview59headless.tar.gz "https://www.paraview.org/paraview-downloads/download.php?submit=Download&version=v5.9&type=binary&os=Linux&downloadFile=ParaView-5.9.1-osmesa-MPI-Linux-Python3.8-64bit.tar.gz"
# extract and rename
tar -xvzf paraview59headless.tar.gz -C paraview59headless
mv "ParaView-5.9.1-osmesa-MPI-Linux-Python3.8-64bit" paraview59headless
# set permissions and relocate it
chmod -R 755 paraview59headless
sudo mv paraview59headless /opt

# now, the path reference in the compute service should be correct

sudo cp distribute-compute.service /etc/systemd/system/
