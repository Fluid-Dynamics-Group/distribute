# update the `distribute` binary
# this script must be run from within the `distribute` 
# repository
RELEASE="release-0.8.0"
git fetch -a &&\
git checkout $RELEASE &&\
git pull && \
cargo install --path ..
