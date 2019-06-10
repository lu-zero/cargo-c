# Cargo C-ABI helpers

## Usage

``` sh
# build the library, create the .h header, create the .pc file
$ cargo c build 
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
