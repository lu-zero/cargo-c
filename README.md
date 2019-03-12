# Cargo C-ABI helpers

## Usage

``` sh
# build the library, create the .h header, create the .pc file
$ cargo c build --prefix=/usr --libdir=/usr/lib64
# install all of it
$ cargo c install --destdir=${D}
```

## Status

- [ ] cli
  - [ ] build command
  - [ ] install command
- [ ] build target
  - [ ] pkg-config generation
  - [ ] header generation (cbindgen integration)
- [ ] `staticlib` support
- [ ] `cdylib` support
- [ ] Extra Cargo.toml keys
