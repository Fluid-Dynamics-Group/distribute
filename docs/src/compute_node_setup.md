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
cd ~
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
```

the most recent `$VERSION` is usually up to date [here](https://github.com/Fluid-Dynamics-Group/distribute/blob/master/install/update.sh)

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
