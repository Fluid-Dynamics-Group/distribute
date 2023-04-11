# after sudo su distribute

cd ~/distribute

# for fish shell
set VERSION "0.14.3"

git fetch -a
git checkout release-$VERSION
git pull

cargo install --path .

rm ~/logs/output.log
systemctl restart distribute-compute
