# distrbute

easy to use distributed computing

## Installing

You will need a recent version of `cargo` and the rust compiler `rustc`. Install instructions are [here](https://www.rust-lang.org/tools/install)

```bash
cargo install --git "https://github.com/fluid-Dynamics-Group/distribute" --force
```

## Documentation:

User documentation (which you are most likely interested in) is hosted on github: https://fluid-dynamics-group.github.io/distribute

Developer documentation is built with

```bash
cargo doc --no-deps --open
```

## Features

If you are using `distribute` as a library, one of the following features *must* be selected for the crate to compile

* `cli` (default)
    * provides command line interface for head node, client node, adding jobs, etc
* `python`
    * use for building python api bindings
    * must specify `--no-default-features` to cargo or `default-features = false` in `Cargo.toml`
* `config`
    * minimal version of `distribute` containing only the schemas and IO for reading / writing configuration files
    * must specify `--no-default-features` to cargo or `default-features = false` in `Cargo.toml`

## Testing

To run unit tests:

```bash
cargo test --lib
```

for 

* unit tests
* integration tests requiring `python3` in `$PATH`

run:

```bash
cargo test
```

for 

* unit tests
* integration tests requiring `python3` in `$PATH`
* integration tests requiring `apptainer` in `$PATH`
    * also requires to build an `apptainer` image to be built: `cd tests/apptainer_local/ && ./build.sh && rm -rf ~/apptainer`

```bash
cargo test --features test-apptainer
```
