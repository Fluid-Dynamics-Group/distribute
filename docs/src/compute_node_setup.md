# Compute Node Setup

First, ensure you have completed the common setup from the [admin setup](./admin_setup.md)

## As the `distribute` user created earlier

create the following directories:

```
mkdir $HOME/data
mkdir $HOME/logs
```

Ensure that `distrbute` is installed for this user (running `distribute` should 
result in some output with no errors)

## While Root

Clone the repo with the correct version of `distribute` you are using

```
git clone https://github.com/Fluid-Dynamics-Group/distribute --depth 1
cd distribute
```

copy the compute service to the system directory:

```
sudo cp install/distribute-compute.service /etc/systemd/system/
```

start the service and enable it at startup:

```
sudo systemctl daemon-reload
sudo systemctl enable distribute-compute
sudo systemctl start distribute-compute
```

Note that if you have deviated from username or folder structure above, `distribute-compute.service` will
have to be updated with those paths since it relies on hard-coded paths.

## Updating

To update, simply reinstall `distribute` and restart the systemd service. On the head node:

```
systemctl restart distribute-server
```

`distribute` requires that all compute nodes and head node have the same version of `distribute` installed.
You can verify that all nodes are correctly setup or updated correctly with 

```
sudo su distribute
cd ~/server
distribute node-status
```

