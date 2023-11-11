use std::path::Path;

use anyhow::*;

use crate::build::CApiConfig;

/// Split a target string to its components
///
/// Because of https://github.com/rust-lang/rust/issues/61558
/// It uses internally `rustc` to validate the string.
#[derive(Clone, Debug)]
pub struct Target {
    pub arch: String,
    // pub vendor: String,
    pub os: String,
    pub env: String,
}

impl Target {
    pub fn new<T: AsRef<std::ffi::OsStr>>(target: T) -> Result<Self, anyhow::Error> {
        let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".into());
        let mut cmd = std::process::Command::new(rustc);

        cmd.arg("--print").arg("cfg");
        cmd.arg("--target").arg(target);

        let out = cmd.output()?;
        if out.status.success() {
            fn match_re(re: regex::Regex, s: &str) -> String {
                re.captures(s)
                    .map_or("", |cap| cap.get(1).unwrap().as_str())
                    .to_owned()
            }

            let arch_re = regex::Regex::new(r#"target_arch="(.+)""#).unwrap();
            // let vendor_re = regex::Regex::new(r#"target_vendor="(.+)""#).unwrap();
            let os_re = regex::Regex::new(r#"target_os="(.+)""#).unwrap();
            let env_re = regex::Regex::new(r#"target_env="(.+)""#).unwrap();

            let s = std::str::from_utf8(&out.stdout).unwrap();

            Ok(Target {
                arch: match_re(arch_re, s),
                // vendor: match_re(vendor_re, s),
                os: match_re(os_re, s),
                env: match_re(env_re, s),
            })
        } else {
            Err(anyhow!("Cannot run {:?}", cmd))
        }
    }

    /// Build a list of linker arguments
    pub fn shared_object_link_args(
        &self,
        capi_config: &CApiConfig,
        libdir: &Path,
        target_dir: &Path,
    ) -> anyhow::Result<Vec<String>> {
        let mut lines = Vec::new();

        let lib_name = &capi_config.library.name;
        let version = &capi_config.library.version;

        let major = version.major;
        let minor = version.minor;
        let patch = version.patch;

        let os = &self.os;
        let env = &self.env;

        let sover = capi_config.library.sover()?;

        if os == "android" {
            lines.push(format!("-Wl,-soname,lib{lib_name}.so"));
        } else if os == "linux"
            || os == "freebsd"
            || os == "dragonfly"
            || os == "netbsd"
            || os == "haiku"
            || os == "illumos"
        {
            lines.push(if capi_config.library.versioning {
                format!("-Wl,-soname,lib{lib_name}.so.{sover}")
            } else {
                format!("-Wl,-soname,lib{lib_name}.so")
            });
        } else if os == "macos" || os == "ios" {
            let line = if capi_config.library.versioning {
                format!("-Wl,-install_name,{1}/lib{0}.{5}.dylib,-current_version,{2}.{3}.{4},-compatibility_version,{5}",
                        lib_name, libdir.display(), major, minor, patch, sover)
            } else {
                format!(
                    "-Wl,-install_name,{1}/lib{0}.dylib",
                    lib_name,
                    libdir.display()
                )
            };
            lines.push(line);
            // Enable larger LC_RPATH and install_name entries
            lines.push("-Wl,-headerpad_max_install_names".to_string());
        } else if os == "windows" && env == "gnu" {
            // This is only set up to work on GNU toolchain versions of Rust
            lines.push(format!(
                "-Wl,--output-def,{}",
                target_dir.join(format!("{lib_name}.def")).display()
            ));
        }

        // Emscripten doesn't support soname or other dynamic linking flags (yet).
        // See: https://github.com/emscripten-core/emscripten/blob/3.1.39/emcc.py#L92-L94
        // else if os == "emscripten"

        Ok(lines)
    }
}
