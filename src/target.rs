use std::env::consts;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use crate::build::CApiConfig;
use anyhow::*;
use cargo::core::compiler::CompileTarget;
use cargo_platform::Cfg;

/// Split a target string to its components
///
/// Because of https://github.com/rust-lang/rust/issues/61558
/// It uses internally `rustc` to validate the string.
#[derive(Clone, Debug)]
pub struct Target {
    pub is_target_overridden: bool,
    pub arch: String,
    // pub vendor: String,
    pub os: String,
    pub env: String,
    pub target: Option<CompileTarget>,
    pub cfg: Vec<Cfg>,
}

impl Target {
    pub fn new<T: AsRef<std::ffi::OsStr> + AsRef<str>>(
        target: Option<T>,
        is_target_overridden: bool,
    ) -> Result<Self> {
        let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".into());
        let mut cmd = std::process::Command::new(rustc);
        let target = target.as_ref();

        cmd.arg("--print").arg("cfg");
        if let Some(target) = target {
            cmd.arg("--target").arg(target);
        }

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

            let lines = s.lines();

            let cfg = lines
                .map(|line| Ok(Cfg::from_str(line)?))
                .collect::<Result<Vec<_>>>()
                .with_context(|| {
                    format!(
                        "failed to parse the cfg from `rustc --print=cfg`, got:\n{}",
                        s
                    )
                })?;

            Ok(Target {
                arch: match_re(arch_re, s),
                // vendor: match_re(vendor_re, s),
                os: match_re(os_re, s),
                env: match_re(env_re, s),
                is_target_overridden,
                target: target.map(|t| CompileTarget::new(t.as_ref())).transpose()?,
                cfg,
            })
        } else {
            Err(anyhow!("Cannot run {:?}", cmd))
        }
    }

    /// Produce the target name, if known
    pub fn name(&self) -> Option<&str> {
        self.target.as_ref().map(|t| t.short_name())
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

        let sover = capi_config.library.sover();

        if os == "android" {
            lines.push(format!("-Wl,-soname,lib{lib_name}.so"));
        } else if os == "linux"
            || os == "freebsd"
            || os == "dragonfly"
            || os == "netbsd"
            || os == "haiku"
            || os == "illumos"
            || os == "openbsd"
            || os == "hurd"
        {
            lines.push(if capi_config.library.versioning {
                format!("-Wl,-soname,lib{lib_name}.so.{sover}")
            } else {
                format!("-Wl,-soname,lib{lib_name}.so")
            });
        } else if os == "macos" || os == "ios" || os == "tvos" || os == "visionos" {
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

        lines
    }

    fn is_freebsd(&self) -> bool {
        self.os.eq_ignore_ascii_case("freebsd")
    }

    fn is_haiku(&self) -> bool {
        self.os.eq_ignore_ascii_case("haiku")
    }

    fn is_windows(&self) -> bool {
        self.os.eq_ignore_ascii_case("windows")
    }

    pub fn default_libdir(&self) -> PathBuf {
        if self.is_target_overridden || self.is_freebsd() {
            return "lib".into();
        }

        if PathBuf::from("/etc/debian_version").exists() {
            let pc = std::process::Command::new("dpkg-architecture")
                .arg("-qDEB_HOST_MULTIARCH")
                .output();
            if let std::result::Result::Ok(v) = pc {
                if v.status.success() {
                    let archpath = String::from_utf8_lossy(&v.stdout);
                    return format!("lib/{}", archpath.trim()).into();
                }
            }
        }

        if consts::ARCH.eq_ignore_ascii_case(&self.arch)
            && consts::OS.eq_ignore_ascii_case(&self.os)
        {
            let usr_lib64 = PathBuf::from("/usr/lib64");
            if usr_lib64.exists() && !usr_lib64.is_symlink() {
                return "lib64".into();
            }
        }

        "lib".into()
    }

    pub fn default_prefix(&self) -> PathBuf {
        if self.is_windows() {
            "c:/".into()
        } else if self.is_haiku() {
            "/boot/system/non-packaged".into()
        } else {
            "/usr/local".into()
        }
    }

    pub fn default_datadir(&self) -> PathBuf {
        if self.is_haiku() {
            return "data".into();
        }
        "share".into()
    }

    pub fn default_includedir(&self) -> PathBuf {
        if self.is_haiku() {
            return "develop/headers".into();
        }
        "include".into()
    }
}
