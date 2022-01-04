# copy and paste this into bash on the `distribute` user
# since the repo will not be cloned yet. 
#
# This script cannot be run by `fish`

git config --global credential.helper store
git clone https://github.com/Fluid-Dynamics-Group/matrix-notify
git clone https://github.com/Fluid-Dynamics-Group/distribute

# this command does not work with `curl` installed through snap
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -- -y

mkdir /home/distribute/data
mkdir /home/distribute/logs

cd distribute
source $HOME/.cargo/env

# installs the `distribute` executable as a binary
cargo install --path .

echo "installing python dependencies" 

pip3 install pandas matplotlib numpy pillow
