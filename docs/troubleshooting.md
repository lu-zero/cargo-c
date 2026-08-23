# Troubleshooting

## Shared libraries are not built on musl systems

When running on a musl-based system (e.g. Alpine Linux), it could be that using
the `cdylib` library type results in the following error (as reported
[here](https://github.com/lu-zero/cargo-c/issues/180)):

> Error: CliError { error: Some(cannot produce cdylib for \<package\> as the target x86_64-unknown-linux-musl does not support these crate types), exit_code: 101 }

This suggests that Rust was not built with `crt-static=false` and it typically
happens if Rust has been installed through rustup.

Shared libraries can be enabled manually in this case, by editing the file
`.cargo/config` like so:

```toml
# .cargo/config

[target.x86_64-unknown-linux-musl]
rustflags = [
    "-C", "target-feature=-crt-static",
]
```

However, it is preferred to install Rust through the system package manager
instead of rustup (e.g. with `apk add rust`), because the provided package
should already handle this (see e.g.
[here](https://git.alpinelinux.org/aports/tree/main/rust/APKBUILD?h=3.19-stable#n232)).

## On Debian-like systems the libdir includes the host triplet by default

In order to accommodate Debian's
[multiarch](https://wiki.debian.org/Multiarch/Implementation) approach the
`cargo-c` default for the `libdir` is `lib/<triplet>` on such systems.
Either pass an explicit `--libdir` or pass `--target` to return to the common
`libdir=lib` default.
