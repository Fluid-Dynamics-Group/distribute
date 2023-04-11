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
* `python` 
* `config`
