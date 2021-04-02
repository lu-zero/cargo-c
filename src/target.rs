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
    ) -> Vec<String> {
        let mut lines = Vec::new();

        let lib_name = &capi_config.library.name;
        let version = &capi_config.library.version;

        let major = version.major;
        let minor = version.minor;
        let patch = version.patch;

        let os = &self.os;
        let env = &self.env;

        if os == "android" {
            lines.push(format!("-Wl,-soname,lib{}.so", lib_name));
        } else if env != "musl"
            && (os == "linux" || os == "freebsd" || os == "dragonfly" || os == "netbsd")
        {
            lines.push(format!("-Wl,-soname,lib{}.so.{}", lib_name, major));
        } else if os == "macos" || os == "ios" {
            let line = format!("-Wl,-install_name,{1}/lib{0}.{2}.{3}.{4}.dylib,-current_version,{2}.{3}.{4},-compatibility_version,{2}",
                    lib_name, libdir.display(), major, minor, patch);
            lines.push(line)
        } else if os == "windows" && env == "gnu" {
            // This is only set up to work on GNU toolchain versions of Rust
            lines.push(format!(
                "-Wl,--output-def,{}",
                target_dir.join(format!("{}.def", lib_name)).display()
            ));
        }

        lines
    }
}
