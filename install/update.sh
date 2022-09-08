# after sudo su distribute

cd ~/distribute

# for fish shell
set VERSION "0.11.3"

git fetch -a
git checkout release-$VERSION
git pull

cargo install --path . --locked


rm ~/logs/output.log
systemctl restart distribute-compute
