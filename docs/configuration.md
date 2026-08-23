# Configuration

Override cargo-c through `package.metadata.capi` in `Cargo.toml`.

```toml
[package.metadata.capi]
# Configures the minimum required cargo-c version. Trying to run with an
# older version causes an error.
min_version = "0.6.10"
```

## Header generation

See also [Header generation](headers.md) for cbindgen vs cheadergen.

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
# Whether to emit library version constants (e.g. `FOO_MAJOR`, `FOO_MINOR` and
# `FOO_PATCH`) at the top of the header file. Enabled by default.
emit_version_constants = true
```

## `pkg-config` file generation

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

## Library generation

```toml
[package.metadata.capi.library]
# Used as the library name and defaults to the crate name. This might get
# prefixed with `lib` depending on the target platform.
name = "new_name"
# dlopen plugin *module*. See docs/plugins.md.
# Requires `install_subdir`. Incompatible with `versioning = true`.
plugin = false
# Used as library version and defaults to the crate version. How this is used
# depends on the target platform.
version = "1.2.3"
# Used to install the library to a subdirectory of `libdir`.
# Required when `plugin = true`.
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

[package.metadata.capi.library.target.'cfg(target_os = "linux")']
# Add target-specific rustflags, the are folded with the main rustflags above
# Note that multiple --version-script support is heavily linker dependent.
rustflags = "-Clink-arg=-Wl,--version-script=assets/version.map"
```

## Custom data install

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

## Notes

Do **not** pass `RUSTFLAGS` that are managed by cargo through other means, (e.g. the flags driven by `[profiles]` or the flags driven by `[target.<>]`), cargo-c effectively builds as if the *target* is always explicitly passed.
