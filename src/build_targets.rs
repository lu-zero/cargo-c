use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::build::{CApiConfig, InstallTarget};
use crate::install::LibType;
use crate::target::Target;

#[derive(Debug, Default, Clone)]
pub struct ExtraTargets {
    pub include: Vec<(PathBuf, PathBuf)>,
    pub data: Vec<(PathBuf, PathBuf)>,
}

impl ExtraTargets {
    pub fn setup(
        &mut self,
        capi_config: &CApiConfig,
        root_dir: &Path,
        out_dir: Option<&Path>,
    ) -> anyhow::Result<()> {
        self.include = extra_targets(&capi_config.install.include, root_dir, out_dir)?;
        self.data = extra_targets(&capi_config.install.data, root_dir, out_dir)?;

        Ok(())
    }
}

fn extra_targets(
    targets: &[InstallTarget],
    root_path: &Path,
    root_output: Option<&Path>,
) -> anyhow::Result<Vec<(PathBuf, PathBuf)>> {
    use itertools::*;
    targets
        .iter()
        .filter_map(|t| match t {
            InstallTarget::Asset(paths) => Some(paths.install_paths(root_path)),
            InstallTarget::Generated(paths) => {
                root_output.map(|root_output| paths.install_paths(root_output))
            }
        })
        .flatten_ok()
        .collect()
}

#[derive(Debug, Clone)]
pub struct BuildTargets {
    pub name: String,
    pub include: Option<PathBuf>,
    pub static_lib: Option<PathBuf>,
    pub shared_lib: Option<PathBuf>,
    pub impl_lib: Option<PathBuf>,
    pub debug_info: Option<PathBuf>,
    pub def: Option<PathBuf>,
    pub pc: PathBuf,
    pub target: Target,
    pub extra: ExtraTargets,
    pub use_meson_naming_convention: bool,
}

impl BuildTargets {
    pub fn new(
        name: &str,
        target: &Target,
        targetdir: &Path,
        libkinds: &[&str],
        capi_config: &CApiConfig,
        use_meson_naming_convention: bool,
    ) -> anyhow::Result<BuildTargets> {
        let pc = targetdir.join(format!("{}.pc", &capi_config.pkg_config.filename));
        let include = if capi_config.header.enabled && capi_config.header.generation {
            let mut header_name = PathBuf::from(&capi_config.header.name);
            header_name.set_extension("h");
            Some(targetdir.join(&header_name))
        } else {
            None
        };

        let lib_name = name;

        let os = &target.os;
        let env = &target.env;

        let (shared_lib, static_lib, impl_lib, debug_info, def) = match (os.as_str(), env.as_str())
        {
            ("none", _)
            | ("linux", _)
            | ("freebsd", _)
            | ("dragonfly", _)
            | ("netbsd", _)
            | ("android", _)
            | ("haiku", _)
            | ("illumos", _)
            | ("emscripten", _) => {
                let static_lib = targetdir.join(format!("lib{lib_name}.a"));
                let shared_lib = targetdir.join(format!("lib{lib_name}.so"));
                (shared_lib, static_lib, None, None, None)
            }
            ("macos", _) | ("ios", _) | ("tvos", _) | ("visionos", _) => {
                let static_lib = targetdir.join(format!("lib{lib_name}.a"));
                let shared_lib = targetdir.join(format!("lib{lib_name}.dylib"));
                (shared_lib, static_lib, None, None, None)
            }
            ("windows", env) => {
                let static_lib = if env == "msvc" {
                    targetdir.join(format!("{lib_name}.lib"))
                } else {
                    targetdir.join(format!("lib{lib_name}.a"))
                };
                let shared_lib = targetdir.join(format!("{lib_name}.dll"));
                let impl_lib = if env == "msvc" {
                    targetdir.join(format!("{lib_name}.dll.lib"))
                } else {
                    targetdir.join(format!("{lib_name}.dll.a"))
                };
                let def = targetdir.join(format!("{lib_name}.def"));
                let pdb = if env == "msvc" {
                    Some(targetdir.join(format!("{lib_name}.pdb")))
                } else {
                    None
                };
                (shared_lib, static_lib, Some(impl_lib), pdb, Some(def))
            }
            _ => unimplemented!("The target {}-{} is not supported yet", os, env),
        };

        let static_lib = if libkinds.contains(&"staticlib") {
            Some(static_lib)
        } else {
            None
        };

        // Bare metal does not support shared objects
        let shared_lib = if libkinds.contains(&"cdylib") && os.as_str() != "none" {
            Some(shared_lib)
        } else {
            None
        };

        Ok(BuildTargets {
            pc,
            include,
            static_lib,
            shared_lib,
            impl_lib,
            debug_info,
            def,
            use_meson_naming_convention,
            name: name.into(),
            target: target.clone(),
            extra: Default::default(),
        })
    }

    fn lib_type(&self) -> LibType {
        LibType::from_build_targets(self)
    }

    pub fn debug_info_file_name(&self, bindir: &Path, libdir: &Path) -> Option<PathBuf> {
        match self.lib_type() {
            // FIXME: Requires setting split-debuginfo to packed and
            // specifying the corresponding file name convention
            // in BuildTargets::new.
            LibType::So | LibType::Dylib => {
                Some(libdir.join(self.debug_info.as_ref()?.file_name()?))
            }
            LibType::Windows => Some(bindir.join(self.debug_info.as_ref()?.file_name()?)),
        }
    }

    pub fn static_output_file_name(&self) -> Option<OsString> {
        match self.lib_type() {
            LibType::Windows => {
                if self.static_lib.is_some() && self.use_meson_naming_convention {
                    Some(format!("lib{}.a", self.name).into())
                } else {
                    Some(self.static_lib.as_ref()?.file_name()?.to_owned())
                }
            }
            _ => Some(self.static_lib.as_ref()?.file_name()?.to_owned()),
        }
    }

    pub fn shared_output_file_name(&self) -> Option<OsString> {
        if self.shared_lib.is_some() && self.use_meson_naming_convention {
            Some(format!("lib{}.dll", self.name).into())
        } else {
            Some(self.shared_lib.as_ref()?.file_name().unwrap().to_owned())
        }
    }
}
