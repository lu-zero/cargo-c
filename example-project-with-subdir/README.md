Example project using `cargo-c`
===============================

For detailed usage instructions, have a look at the
[Github workflow configuration](../.github/workflows/example-project-with-subdir.yml).

Note that `cargo install --path .` is used to install `cargo-c`
from the locally cloned Git repository.
If you want to install the latest release from
[crates.io](https://crates.io/crates/cargo-c),
you should use this instead:

    cargo install cargo-c

Running `cargo cinstall` will create the C header file `example_project_with_subdir.h`.
This file will contain the comments from the file [`capi.rs`](src/capi.rs).
It will be installed in a subdirectory (as specified in [`Cargo.toml`](Cargo.toml))
of the default directory (e.g. `/usr/local/include`),
together with additional files specified in [`build.rs`](build.rs).

Run `cargo doc --open` to view the documentation of the Rust code.
