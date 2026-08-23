# Plugin directory vs plugin module

`install_subdir` and `plugin` are two different layers. Mixing them up is how
unprefixed, versioned libraries end up in the default libdir.

| | C library (default) | Plugin directory (`install_subdir` only) | Plugin module (`plugin = true`) |
|---|---|---|---|
| Example | rav1e, rustls-ffi | [gst-plugins-rs](https://gitlab.freedesktop.org/gstreamer/gst-plugins-rs) | PAM, Apache |
| Prefix | `lib{name}` | `lib{name}` | `{name}` |
| Versioning | default on | usually off | forced off |
| Install | `$libdir` | `$libdir/$subdir` | `$libdir/$subdir` only |
| `.a` / `.pc` / headers / implib | yes | yes (GST static) | skipped (headers default off) |
| SONAME / install_name | `lib{name}.so.*` | same | `{name}.so` |

`library.name` is always the unprefixed identifier. Only `plugin = true` stops
cargo-c from adding `lib`. rustc still emits `lib$name.so` in the build
directory; only the installed name and the linker soname drop the prefix.

## Plugin directory

A C library that happens to be loaded from a subdirectory of `libdir`.
GStreamer is the reference:

```toml
[lib]
name = "gstcdg"
crate-type = ["cdylib", "rlib"]

[features]
capi = []

[package.metadata.capi.header]
enabled = false

[package.metadata.capi.library]
install_subdir = "gstreamer-1.0"
versioning = false
import_library = false
```

`cargo cinstall --prefix=/usr` installs `libgstcdg.so` into
`/usr/lib/gstreamer-1.0`. The `lib` prefix stays; `.a` / `.pc` are still
produced for static linking. On Windows, `install_subdir` already copies the
rustc DLL name into that subdirectory instead of `bindir`.

Do **not** set `plugin = true` for this case.

## Plugin module

A dlopen module, not something linked with `-lfoo`. PAM, Apache modules, and
similar loaders look up a basename that is the public name:

```toml
[lib]
name = "pam_cgroup"
crate-type = ["cdylib", "rlib"]

[features]
capi = []

[package.metadata.capi.library]
plugin = true
install_subdir = "security"
name = "pam_cgroup"
```

That installs `$libdir/security/pam_cgroup.so` (no `lib` prefix, no
`pam_cgroup.so.0.1.0`). The linker soname / install_name matches the installed
file. `.a`, `.pc`, and Windows import libraries are not installed; headers
default to off unless `package.metadata.capi.header.enabled` is set explicitly.

`plugin = true` without a non-empty `install_subdir`, or together with
`versioning = true`, is an error.

NSS-style `libnss_files.so.2` is a versioned, prefixed library that happens to
be dlopened: leave `plugin` unset.
