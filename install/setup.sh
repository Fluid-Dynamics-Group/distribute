git config --global credential.helper store
git clone https://github.com/Fluid-Dynamics-Group/matrix-notify
git clone https://github.com/Fluid-Dynamics-Group/distribute

curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -- -y

mkdir /home/distribute/data
mkdir /home/distribute/logs

cd distribute
source $HOME/.cargo/env

cargo install --path .
