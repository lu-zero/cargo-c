# Cargo C-ABI helpers

[![LICENSE](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

[cargo](https://doc.rust-lang.org/cargo) applet to build and install C-ABI compatibile dynamic and static libraries.

It produces and installs a correct [pkg-config](https://www.freedesktop.org/wiki/Software/pkg-config/) file, a static library and a dynamic library, and a C header to be used by any C (and C-compatible) software.

## Usage
```
# build the library, create the .h header, create the .pc file
$ cargo cbuild --destdir=${D} --prefix=/usr --libdir=/usr/lib64
```
``` sh
# build the library, create the .h header, create the .pc file and install all of it
$ cargo cinstall --destdir=${D} --prefix=/usr --libdir=/usr/lib64
```

## Status

- [x] cli
  - [x] install command
  - [x] cargo applet support
- [x] build targets
  - [x] pkg-config generation
  - [x] header generation (cbindgen integration)
- [x] `staticlib` support
- [x] `cdylib` support
- [ ] Extra Cargo.toml keys
