use std::path::{Path, PathBuf};

use crate::build::{CApiConfig, InstallTarget};
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
    pub include: Option<PathBuf>,
    pub static_lib: Option<PathBuf>,
    pub shared_lib: Option<PathBuf>,
    pub impl_lib: Option<PathBuf>,
    pub def: Option<PathBuf>,
    pub pc: PathBuf,
    pub target: Target,
    pub extra: ExtraTargets,
}

impl BuildTargets {
    pub fn new(
        name: &str,
        target: &Target,
        targetdir: &Path,
        libkinds: &[&str],
        capi_config: &CApiConfig,
    ) -> anyhow::Result<BuildTargets> {
        let pc = targetdir.join(format!("{}.pc", &capi_config.pkg_config.filename));
        let include = if capi_config.header.enabled {
            let mut header_name = PathBuf::from(&capi_config.header.name);
            header_name.set_extension("h");
            Some(targetdir.join(&header_name))
        } else {
            None
        };

        let lib_name = name;

        let os = &target.os;
        let env = &target.env;

        let (shared_lib, static_lib, impl_lib, def) = match (os.as_str(), env.as_str()) {
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
                (shared_lib, static_lib, None, None)
            }
            ("macos", _) | ("ios", _) | ("tvos", _) => {
                let static_lib = targetdir.join(format!("lib{lib_name}.a"));
                let shared_lib = targetdir.join(format!("lib{lib_name}.dylib"));
                (shared_lib, static_lib, None, None)
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
                (shared_lib, static_lib, Some(impl_lib), Some(def))
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
            def,
            target: target.clone(),
            extra: Default::default(),
        })
    }
}
