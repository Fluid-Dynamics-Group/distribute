# after sudo su distribute

cd ~/distribute

# for fish shell
set VERSION "0.11.1"

git fetch -a
git checkout release-$VERSION
git pull

cargo install --path . --locked

systemctl restart distribute-compute
