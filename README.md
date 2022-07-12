# distrbute

easy to use distributed computing

## Installing

You will need a recent version of `cargo` and the rust compiler `rustc`. Install instructions are [here](https://www.rust-lang.org/tools/install)

```
cargo install --git https://github.com/fluid-Dynamics-Group/distribute --force
```

## Documentation:

User documentation (which you are most likely interested in) is hosted on github: https://fluid-dynamics-group.github.io/distribute-docs

Developer documentation is built with 

```
cargo doc --no-deps --open
```

## Releasing a New Version

on master branch of a development machine:

```
git checkout -b release-$VERSION
git push --set-upstream origin release-$VERSION
```

then, on each compute node:

```
sudo su distribute

git fetch -a
git checkout release-$VERSION
git pull

cargo install --path .

systemctl restart distribute-compute
```
