# Header generation

By default cargo-c generates a C header with [cbindgen](https://github.com/mozilla/cbindgen).
See [`package.metadata.capi.header`](configuration.md#header-generation) for the
keys that control the filename, install subdirectory, and whether a header is
generated at all.

Add a [`cbindgen.toml`](https://github.com/eqrion/cbindgen/blob/master/docs.md#cbindgentoml)
with at least an include guard; set `language = "C"` if you do not want C++.

If the bindings live in a separate crate and the header is hand-written or
produced elsewhere:

- Keep a `capi` feature so cargo-c can identify the package in a workspace.
- Set `package.metadata.capi.header.generation` to `false` (or `enabled = false`
  to skip headers completely).
- Optionally install a pre-generated header with
  [`package.metadata.capi.install.include`](configuration.md#custom-data-install).

## Using cheadergen

Instead of `cbindgen`, you can use [`cheadergen`](https://cheadergen.com/) to
generate C headers. cheadergen uses the `rustdoc-json` output from the Rust
compiler to generate more accurate bindings, including doc comments from your
Rust code.

1. Install cheadergen and its required nightly toolchain:

   ```sh
   cargo install cheadergen_cli
   rustup install nightly -c rust-docs-json
   ```

2. Disable cargo-c's automatic header generation (which uses cbindgen):

   ```toml
   [package.metadata.capi.header]
   generation = false
   ```

3. Add a build script that runs cheadergen before cargo-c:

   ```rust
   // build.rs
   fn main() {
       // Generate header with cheadergen
       std::process::Command::new("cheadergen")
           .arg("generate")
           .arg("--output")
           .arg("include/my_crate.h")
           .status()
           .expect("cheadergen failed");
   }
   ```

4. Configure cargo-c to install the pre-generated header:

   ```toml
   [package.metadata.capi.install.include]
   asset = [{ from = "include/my_crate.h", to = "" }]
   ```

For more details on cheadergen, see the
[official documentation](https://cheadergen.com/getting-started/install).
