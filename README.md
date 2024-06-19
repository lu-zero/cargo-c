# Cargo C-ABI helpers

[![LICENSE](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Crates.io](https://img.shields.io/crates/v/cargo-c.svg)](https://crates.io/crates/cargo-c)
[![Build Status](https://github.com/lu-zero/cargo-c/workflows/Rust/badge.svg)](https://github.com/lu-zero/cargo-c/actions?query=workflow:Rust)
[![cargo-c chat](https://img.shields.io/badge/zulip-join_chat-brightgreen.svg)](https://rust-av.zulipchat.com/#narrow/stream/254255-cargo-c)
[![dependency status](https://deps.rs/repo/github/lu-zero/cargo-c/status.svg)](https://deps.rs/repo/github/lu-zero/cargo-c)

[cargo](https://doc.rust-lang.org/cargo) applet to build and install C-ABI compatible dynamic and static libraries.

It produces and installs a correct [pkg-config](https://www.freedesktop.org/wiki/Software/pkg-config/) file, a static library and a dynamic library, and a C header to be used by any C (and C-compatible) software.

## Installation
**cargo-c** may be installed from [crates.io](https://crates.io/crates/cargo-c).
``` sh
cargo install cargo-c
```

The `rustc` version supported is the same as the one supported by the `cargo` version embedded in the package version, or as set in the
[rust-version](https://doc.rust-lang.org/cargo/reference/manifest.html#the-rust-version-field) field.

You must have the **cargo** build [requirements](https://github.com/rust-lang/cargo#compiling-from-source) satisfied in order to build **cargo-c**:
* `git`
* `pkg-config` (on Unix, used to figure out the host-provided headers/libraries)
* `curl` (on Unix)
* OpenSSL headers (only for Unix, this is the `libssl-dev` package on deb-based distributions)

You may pass `--features=vendored-openssl` if you have problems building openssl-sys using the host-provided OpenSSL.

``` sh
cargo install cargo-c --features=vendored-openssl
```

## Usage
``` sh
# build the library, create the .h header, create the .pc file
$ cargo cbuild --destdir=${D} --prefix=/usr --libdir=/usr/lib64
```
``` sh
# build the library, create the .h header, create the .pc file, build and run the tests
$ cargo ctest
```
``` sh
# build the library, create the .h header, create the .pc file and install all of it
$ cargo cinstall --destdir=${D} --prefix=/usr --libdir=/usr/lib64
```

For a more in-depth explanation of how `cargo-c` works and how to use it for
your crates, read [Building Crates so they Look Like C ABI Libraries][dev.to].

### The TL;DR:

- [Create][diff-1] a `capi.rs` with the C-API you want to expose and use
  ~~`#[cfg(cargo_c)]`~~`#[cfg(feature="capi")]` to hide it when you build a normal rust library.
- [Make sure][diff-2] you have a lib target and if you are using a workspace
  the first member is the crate you want to export, that means that you might
  have [to add a "." member at the start of the list][diff-3].
- ~~Since Rust 1.38, also add "staticlib" to the "lib" `crate-type`.~~ Do not specify the `crate-type`, cargo-c will add the correct library target by itself.
- You may use the feature `capi` to add C-API-specific optional dependencies.
  > **NOTE**: It must be always present in `Cargo.toml`
- Remember to [add][diff-4] a [`cbindgen.toml`][cbindgen-toml] and fill it with
  at least the include guard and probably you want to set the language to C (it
  defaults to C++)
- Once you are happy with the result update your documentation to tell the user
  to install `cargo-c` and do `cargo cinstall --prefix=/usr
  --destdir=/tmp/some-place` or something along those lines.

[diff-1]: https://github.com/RustAudio/lewton/pull/50/commits/557cb4ce35beedf6d6bfaa481f29936094a71669
[diff-2]: https://github.com/RustAudio/lewton/pull/50/commits/e7ea8fff6423213d1892e86d51c0c499d8904dc1
[diff-3]: https://github.com/xiph/rav1e/pull/1381/commits/7d558125f42f4b503bcdcda5a82765da76a227e0#diff-80398c5faae3c069e4e6aa2ed11b28c0R94
[diff-4]: https://github.com/RustAudio/lewton/pull/51/files
[cbindgen-toml]: https://github.com/eqrion/cbindgen/blob/master/docs.md#cbindgentoml

## Advanced
You may override various aspects of `cargo-c` via settings in `Cargo.toml` under the `package.metadata.capi` key

```toml
[package.metadata.capi]
# Configures the minimum required cargo-c version. Trying to run with an
# older version causes an error.
min_version = "0.6.10"
```

### Header Generation

```toml
[package.metadata.capi.header]
# Used as header file name. By default this is equal to the crate name.
# The name can be with or without the header filename extension `.h`
name = "new_name"
# Install the header into a subdirectory with the name of the crate. This
# is enabled by default, pass `false` or "" to disable it.
subdirectory = "libfoo-2.0/foo"
# Generate the header file with `cbindgen`, or copy a pre-generated header
# from the `assets` subdirectory. By default a header is generated.
generation = true
# Can be use to disable header generation completely.
# This can be used when generating dynamic modules instead of an actual library.
enabled = true
```

### `pkg-config` File Generation

```toml
[package.metadata.capi.pkg_config]
# Used as the package name in the pkg-config file and defaults to the crate name.
name = "libfoo"
# Used as the pkg-config file name and defaults to the crate name.
filename = "libfoo-2.0"
# Used as the package description in the pkg-config file and defaults to the crate description.
description = "some description"
# Used as the package version in the pkg-config file and defaults to the crate version.
version = "1.2.3"
# Used as the Requires field in the pkg-config file, if defined
requires = "gstreamer-1.0, gstreamer-base-1.0"
# Used as the Requires.private field in the pkg-config file, if defined
requires_private = "gobject-2.0, glib-2.0 >= 2.56.0, gmodule-2.0"
# Strip the include search path from the last n components, useful to support installing in a
# subdirectory but then include with the path. By default it is 0.
strip_include_path_components = 1

```

### Library Generation

```toml
[package.metadata.capi.library]
# Used as the library name and defaults to the crate name. This might get
# prefixed with `lib` depending on the target platform.
name = "new_name"
# Used as library version and defaults to the crate version. How this is used
# depends on the target platform.
version = "1.2.3"
# Used to install the library to a subdirectory of `libdir`.
install_subdir = "gstreamer-1.0"
# Used to disable versioning links when installing the dynamic library
versioning = false
# Instead of using semver, select a fixed number of version components for your SONAME version suffix:
# Setting this to 1 with a version of 0.0.0 allows a suffix of `.so.0`
# Setting this to 3 always includes the full version in the SONAME (indicate any update is ABI breaking)
#version_suffix_components = 2
# Add `-Cpanic=abort` to the RUSTFLAGS automatically, it may be useful in case
# something might panic in the crates used by the library.
rustflags = "-Cpanic=abort"
# Used to disable the generation of additional import library file in platforms
# that have the concept such as Windows
import_library = false
```

### Custom data install
```toml
[package.metadata.capi.install.data]
# Used to install the data to a subdirectory of `datadir`. By default it is the same as `name`
subdirectory = "foodata"
# Copy the pre-generated data files found in {root_dir}/{from} to {datadir}/{to}/{matched subdirs}
# If {from} is a single path instead of a glob, the destination is {datapath}/{to}.
# datapath is {datadir}/{subdirectory}
asset = [{from="pattern/with/or/without/**/*", to="destination"}]
# Copy the pre-generated data files found in {OUT_DIR}/{from} to {includedir}/{to}/{matched subdirs}
# If {from} is a single path instead of a glob, the destination is {datapath}/{to}.
# datapath is {datadir}/{subdirectory}
generated = [{from="pattern/with/or/without/**/*", to="destination"}]

[package.metadata.capi.install.include]
# Copy the pre-generated includes found in {root_dir}/{from} to {includedir}/{to}/{matched subdirs}
# If {from} is a single path instead of a glob, the destination is {includepath}/{to}.
# includepath is {includedir}/{header.subdirectory}
asset = [{from="pattern/with/or/without/**/*", to="destination"}]
# Copy the pre-generated includes found in {OUT_DIR}/{from} to {includedir}/{to}/{matched subdirs}
# If {from} is a single path instead of a glob, the destination is {includedpath}/{to}.
# includepath is {includedir}/{header.subdirectory}
generated = [{from="pattern/with/or/without/**/*", to="destination"}]
```

### Notes

Do **not** pass `RUSTFLAGS` that are managed by cargo through other means, (e.g. the flags driven by `[profiles]` or the flags driven by `[target.<>]`), cargo-c effectively builds as if the *target* is always explicitly passed.

## Users

- [ebur128](https://github.com/sdroege/ebur128#c-api)
- [gcode-rs](https://github.com/Michael-F-Bryan/gcode-rs)
- [gst-plugins-rs](https://gitlab.freedesktop.org/gstreamer/gst-plugins-rs)
- [lewton](https://github.com/RustAudio/lewton)
- [libdovi](https://github.com/quietvoid/dovi_tool/tree/main/dolby_vision#libdovi-c-api)
- [libimagequant](https://github.com/ImageOptim/libimagequant#building-with-cargo-c)
- [rav1e](https://github.com/xiph/rav1e)
- [rustls-ffi](https://github.com/rustls/rustls-ffi)
- [sled](https://github.com/spacejam/sled/tree/master/bindings/sled-native)
- [pathfinder](https://github.com/servo/pathfinder#c)
- [udbserver](https://github.com/bet4it/udbserver)

## Status

- [x] cli
  - [x] build command
  - [x] install command
  - [x] test command
  - [x] cargo applet support
- [x] build targets
  - [x] pkg-config generation
  - [x] header generation (cbindgen integration)
- [x] `staticlib` support
- [x] `cdylib` support
- [x] Generate version information in the header
  - [ ] Make it tunable
- [x] Extra Cargo.toml keys
- [x] Better status reporting

[dev.to]: https://dev.to/luzero/building-crates-so-they-look-like-c-abi-libraries-1ibn
[using]: https://dev.to/luzero/building-crates-so-they-look-like-c-abi-libraries-1ibn#using-cargoc

## Availability

[![Packaging status](https://repology.org/badge/vertical-allrepos/cargo-c.svg)](https://repology.org/project/cargo-c/versions)

## Troubleshooting

### Shared libraries are not built on musl systems

When running on a musl-based system (e.g. Alpine Linux), it could be that using the `cdylib` library type results in the following error (as reported [here](https://github.com/lu-zero/cargo-c/issues/180)):

> Error: CliError { error: Some(cannot produce cdylib for <package> as the target x86_64-unknown-linux-musl does not support these crate types), exit_code: 101 }

This suggests that Rust was not built with `crt-static=false` and it typically happens if Rust has been installed through rustup.

Shared libraries can be enabled manually in this case, by editing the file `.cargo/config` like so:

```toml
# .cargo/config

[target.x86_64-unknown-linux-musl]
rustflags = [
    "-C", "target-feature=-crt-static",
]
```

However, it is preferred to install Rust through the system package manager instead of rustup (e.g. with `apk add rust`), because the provided package should already handle this (see e.g. [here](https://git.alpinelinux.org/aports/tree/main/rust/APKBUILD?h=3.19-stable#n232)).

### On Debian-like system the libdir includes the host triplet by default

In order to accomodate Debian's [multiarch](https://wiki.debian.org/Multiarch/Implementation) approach the `cargo-c` default for the `libdir` is `lib/<triplet>` on such system.
Either pass an explicit `--libdir` or pass `--target` to return to the common `libdir=lib` default.

## Acknowledgements

This software has been partially developed in the scope of the H2020 project SIFIS-Home with GA n. 952652.
