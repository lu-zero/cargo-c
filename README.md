# Cargo C-ABI helpers

[![LICENSE](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![dependency status](https://deps.rs/repo/github/lu-zero/cargo-c/status.svg)](https://deps.rs/repo/github/lu-zero/cargo-c)

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

The TL;DR:

- [Create][diff-1] a `capi.rs` with the C-API you want to expose and use
  `#[cfg(cargo_c)]` to hide it when you build a normal rust library.
- [Make sure][diff-2] you have a lib target and if you are using a workspace
  the first member is the crate you want to export, that means that you might
  have [to add a "." member at the start of the list][diff-3].
- Remember to [add][diff-4] a [`cbindgen.toml`][cbinden-toml] and fill it with
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
- [ ] Extra Cargo.toml keys
- [ ] Better status reporting

[dev.to]: https://dev.to/luzero/building-crates-so-they-look-like-c-abi-libraries-1ibn
[using]: https://dev.to/luzero/building-crates-so-they-look-like-c-abi-libraries-1ibn#using-cargoc
