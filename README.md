# Cargo C-ABI helpers

[![LICENSE](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

[cargo](https://doc.rust-lang.org/cargo) apple to build and install C-ABI compatibile dynamic and static libraries.

It produces and installs a correct [pkg-config](https://www.freedesktop.org/wiki/Software/pkg-config/) file, a static library and a dynamic library, and a C header to be used by any C (and C-compatible) software.

## Usage

``` sh
# build the library, create the .h header, create the .pc file
$ cargo c build
```
```
# install all of it
$ cargo c install --destdir=${D} --prefix=/usr --libdir=/usr/lib64
```

## Status

- [x] cli
  - [x] build command
  - [x] install command
- [x] build target
  - [x] pkg-config generation
  - [x] header generation (cbindgen integration)
- [x] `staticlib` support
- [x] `cdylib` support
- [ ] Extra Cargo.toml keys
