# Head Node Setup

First, ensure you have completed the common setup from the [admin setup](./admin_setup.md)

The ip address of the head node is important: it is the IP that all users will use to 
send jobs to the cluster. Ensure this is documented somewhere.

## As the `distribute` user created earlier

when logged into the user `distribute`, run

```
mkdir $HOME/server
mkdir $HOME/server/results
mkdir $HOME/server/tmp
```

You will need a `distribute-nodes.yaml` file to specify to distribute what the IP addresses 
of each compute node is in the cluster, and what software / hardware is available (`capabilities`).
[here](https://github.com/Fluid-Dynamics-Group/distribute/blob/master/static/example-nodes.yaml)
is a currently compiling example from the master branch. Ensure that you use descriptive names for each
node as they will appear in the logs. Place this file at `/home/distribute/server/distribute-nodes.yaml`.

Ensure that `distrbute` is installed for this user (running `distribute` should 
result in some output with no errors)

## While Root

Clone the repo with the correct version of `distribute` you are using

```
git clone https://github.com/Fluid-Dynamics-Group/distribute --depth 1
cd distribute
```

copy the server service to the system directory:

```
sudo cp install/distribute-server.service /etc/systemd/system/
```

start the service and enable it at startup:

```
sudo systemctl daemon-reload
sudo systemctl enable distribute-server
sudo systemctl start distribute-server
```

Note that if you have deviated from username or folder structure above, `distribute-server.service` will
have to be updated with those paths since it relies on hard-coded paths.

## Updating

To update, simply reinstall `distribute` and restart the systemd service. On the compute node:

```
systemctl restart distribute-compute
```
