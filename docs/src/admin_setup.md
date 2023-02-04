# Admin Setup

This is a guide for the cluster administrator to setup `distribute` on each computer. Computers
with `distribute` are required to be linux-based.

systemd init files are provided so that `distribute` runs at startup and can be restarted 
in the background easily.

## Initial Setup

### Creating a New Linux User

```
set USERNAME distribute

# create the user `distribute
sudo useradd $USERNAME -m

# set the password for this user
sudo passwd $USERNAME

# login to the user using sudo privileges
sudo su $USERNAME

# change the shell to fish (could also use bash)
chsh
/usr/bin/bash
```

### Fetching IP Address

```
ip --brief addr
```

an example output here is

```
lo               UNKNOWN        127.0.0.1/8 ::1/128
enp0s31f6        UP             192.168.1.136/24 fe80::938:b383:b4d1:6dc/64
wlp4s0           DOWN
br-29d4537f30a4  DOWN           172.18.0.1/16
docker0          DOWN           172.17.0.1/16 fe80::42:7fff:fe80:d0cc/64
```

in which case, `192.168.1.136` would be the IP of the machine you use
