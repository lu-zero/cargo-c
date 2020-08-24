# Cargo C-ABI helpers

[![LICENSE](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![dependency status](https://deps.rs/repo/github/lu-zero/cargo-c/status.svg)](https://deps.rs/repo/github/lu-zero/cargo-c)
![Crates.io](https://img.shields.io/crates/v/cargo-c.svg)
[![Build Status](https://github.com/lu-zero/cargo-c/workflows/Rust/badge.svg)](https://github.com/lu-zero/cargo-c/actions?query=workflow:Rust)

[cargo](https://doc.rust-lang.org/cargo) applet to build and install C-ABI compatibile dynamic and static libraries.

It produces and installs a correct [pkg-config](https://www.freedesktop.org/wiki/Software/pkg-config/) file, a static library and a dynamic library, and a C header to be used by any C (and C-compatible) software.

## Usage
``` sh
# build the library, create the .h header, create the .pc file
$ cargo cbuild --destdir=${D} --prefix=/usr --libdir=/usr/lib64
```
``` sh
# build the library, create the .h header, create the .pc file and install all of it
$ cargo cinstall --destdir=${D} --prefix=/usr --libdir=/usr/lib64
```

For a more in-depth explanation of how `cargo-c` works and how to use it for
your crates, read [Building Crates so they Look Like C ABI Libraries][dev.to].

### The TL;DR:

- [Create][diff-1] a `capi.rs` with the C-API you want to expose and use
  `#[cfg(cargo_c)]` to hide it when you build a normal rust library.
- [Make sure][diff-2] you have a lib target and if you are using a workspace
  the first member is the crate you want to export, that means that you might
  have [to add a "." member at the start of the list][diff-3].
- ~~Since Rust 1.38, also add "staticlib" to the "lib" `crate-type`.~~ Do not specify the `crate-type`, cargo-c will add the correct library target by itself.
- You may use the feature `capi` to add C-API-specific optional dependencies.
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
# is enabled by default
subdirectory = true
# Generate the header file with `cbindgen`, or copy a pre-generated header
# from the `assets` subdirectory. By default a header is generated.
generation = true
```

### `pkg-config` File Generation

```toml
[package.metadata.capi.pkg_config]
# Used as the package name in the pkg-config file and defaults to the crate name.
name = "libfoo"
# Used as the package description in the pkg-config file and defaults to the crate description.
description = "some description"
# Used as the package version in the pkg-config file and defaults to the crate version.
version = "1.2.3"
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
```

## Users

- [gcode-rs](https://github.com/Michael-F-Bryan/gcode-rs)
- [lewton](https://github.com/RustAudio/lewton)
- [rav1e](https://github.com/xiph/rav1e)
- [sled](https://github.com/spacejam/sled/tree/master/bindings/sled-native)
- [pathfinder](https://github.com/servo/pathfinder#c)

## Status

- [x] cli
  - [x] build command
  - [x] install command
  - [x] cargo applet support
- [x] build targets
  - [x] pkg-config generation
  - [x] header generation (cbindgen integration)
- [x] `staticlib` support
- [x] `cdylib` support
- [x] Generate version information in the header
  - [ ] Make it tunable
- [ ] Extra Cargo.toml keys
- [x] Better status reporting

[dev.to]: https://dev.to/luzero/building-crates-so-they-look-like-c-abi-libraries-1ibn
[using]: https://dev.to/luzero/building-crates-so-they-look-like-c-abi-libraries-1ibn#using-cargoc
