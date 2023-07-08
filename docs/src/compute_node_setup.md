# Compute Node Setup

First, ensure you have completed the common setup from the [admin setup](./admin_setup.md)

## As the `distribute` user created earlier

create the following directories:

```bash
mkdir $HOME/data
mkdir $HOME/logs
```

Clone `distribute` from the github repo and install the identical version as the head node.

```bash
git clone https://github.com/Fluid-Dynamics-Group/distribute --depth 1
cd distribute
```

then, to update to `$VERSION` (fish shell syntax):

```bash
# for fish shell
set VERSION "0.14.5"

git fetch -a
git checkout release-$VERSION
git pull

cargo install --path .

rm ~/logs/output.log
systemctl restart distribute-compute
```

the most recent `$VERSION` is usually up to date [here](https://github.com/Fluid-Dynamics-Group/distribute/blob/master/install/update.sh)

### Verifying installation

Enter the `distribute` source code and run library tests with `cargo`. Note that you
will have to complete the `docker` setup described below for all tests to pass.

```bash
cd ~/distribute
cargo test --lib
```

## While Root

Clone the repo with the correct version of `distribute` you are using

```bash
git clone https://github.com/Fluid-Dynamics-Group/distribute --depth 1
cd distribute
```

copy the compute service to the system directory:

```bash
sudo cp install/distribute-compute.service /etc/systemd/system/
```

start the service and enable it at startup:

```bash
sudo systemctl daemon-reload
sudo systemctl enable distribute-compute
sudo systemctl start distribute-compute
```

Note that if you have deviated from username or folder structure above, `distribute-compute.service` will
have to be updated with those paths since it relies on hard-coded paths.

### Docker Setup

As root, you must allow `docker` to be executed by the `distribute` user without `sudo`. This is
done because the `distribute` executable explicitly avoids running with root privileges. Without 
allowing `docker` to run rootless, you would be unable to start `docker` containers without distribute
being run as root. See [this documentation](https://askubuntu.com/questions/477551/how-can-i-use-docker-without-sudo#answer-477554)
on the security implications of this.

As root, create `docker` group:

```bash
sudo groupadd docker
```

add the user `distribute` to the docker group

```bash
sudo gpasswd -a distribute docker
newgrp docker
```

and restart the machine.

## Updating

To update, simply reinstall `distribute` and restart the systemd service. On the compute node (for a fixed version at the time of writing):

```bash
# for fish shell
set VERSION "0.14.5"

git fetch -a
git checkout release-$VERSION
git pull

cargo install --path .

rm ~/logs/output.log
systemctl restart distribute-compute
```

the most recent `$VERSION` is usually up to date [here](https://github.com/Fluid-Dynamics-Group/distribute/blob/master/install/update.sh).
Then, restart the systemd service so that is uses the new version of distribute:

```bash
systemctl restart distribute-server
```
