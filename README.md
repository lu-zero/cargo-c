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
This is the ideal setup for a project that wants to keep their C-API within the main crate:
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

If you plan to keep the bindings as a separate crate and do not need to autogenerate the headers you may just [populate Cargo.toml][diff-5]:
- Add a `capi` feature, since it is used by cargo-c to identify packages that has to be built as C-libraries within a workspace.
- Set the entry in `package.metadata.capi.header.generation` to `false`.
- Optionally override the path to the header to a custom one instead of the default one.

[diff-1]: https://github.com/RustAudio/lewton/pull/50/commits/557cb4ce35beedf6d6bfaa481f29936094a71669
[diff-2]: https://github.com/RustAudio/lewton/pull/50/commits/e7ea8fff6423213d1892e86d51c0c499d8904dc1
[diff-3]: https://github.com/xiph/rav1e/pull/1381/commits/7d558125f42f4b503bcdcda5a82765da76a227e0#diff-80398c5faae3c069e4e6aa2ed11b28c0R94
[diff-4]: https://github.com/RustAudio/lewton/pull/51/files
[diff-5]: https://github.com/linebender/resvg/commit/c0777c7ce26bf40efed7ba38d0a70e5af83feb78
[cbindgen-toml]: https://github.com/eqrion/cbindgen/blob/master/docs.md#cbindgentoml

## Documentation

- [Configuration](docs/configuration.md) — `package.metadata.capi` keys
- [Plugin directory vs plugin module](docs/plugins.md)
- [Header generation](docs/headers.md) — cbindgen and cheadergen
- [Troubleshooting](docs/troubleshooting.md)

## Users

- [ebur128](https://github.com/sdroege/ebur128#c-api)
- [gcode-rs](https://github.com/Michael-F-Bryan/gcode-rs)
- [gst-plugins-rs](https://gitlab.freedesktop.org/gstreamer/gst-plugins-rs)
- [lewton](https://github.com/RustAudio/lewton)
- [libdovi](https://github.com/quietvoid/dovi_tool/tree/main/dolby_vision#libdovi-c-api)
- [libimagequant](https://github.com/ImageOptim/libimagequant#building-with-cargo-c)
- [librsvg](https://github.com/GNOME/librsvg/blob/main/rsvg/meson.build)
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

## Acknowledgements

This software has been partially developed in the scope of the H2020 project SIFIS-Home with GA n. 952652.
