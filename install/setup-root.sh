# now, the path reference in the compute service should be correct

sudo cp distribute-compute.service /etc/systemd/system/

# systemctl stuff - reload the file, make it run at startup
# and start it now
sudo systemctl daemon-reload
sudo systemctl enable distribute-compute
sudo systemctl start distribute-compute

echo "setting up new user `distribute` without root access"
sudo useradd -m distribute
echo "setting the password for `distribute`"
sudo passwd distribute
