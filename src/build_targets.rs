use std::path::{Path, PathBuf};

use crate::build::CApiConfig;
use crate::target::Target;

#[derive(Debug)]
pub struct BuildTargets {
    pub include: Option<PathBuf>,
    pub static_lib: Option<PathBuf>,
    pub shared_lib: Option<PathBuf>,
    pub impl_lib: Option<PathBuf>,
    pub def: Option<PathBuf>,
    pub pc: PathBuf,
    pub target: Target,
}

impl BuildTargets {
    pub fn new(
        name: &str,
        target: &Target,
        targetdir: &Path,
        libkinds: &[&str],
        capi_config: &CApiConfig,
    ) -> BuildTargets {
        let pc = targetdir.join(&format!("{}.pc", name));
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
            | ("android", _) => {
                let static_lib = targetdir.join(&format!("lib{}.a", lib_name));
                let shared_lib = targetdir.join(&format!("lib{}.so", lib_name));
                (shared_lib, static_lib, None, None)
            }
            ("macos", _) | ("ios", _) => {
                let static_lib = targetdir.join(&format!("lib{}.a", lib_name));
                let shared_lib = targetdir.join(&format!("lib{}.dylib", lib_name));
                (shared_lib, static_lib, None, None)
            }
            ("windows", ref env) => {
                let static_lib = if *env == "msvc" {
                    targetdir.join(&format!("{}.lib", lib_name))
                } else {
                    targetdir.join(&format!("lib{}.a", lib_name))
                };
                let shared_lib = targetdir.join(&format!("{}.dll", lib_name));
                let impl_lib = if *env == "msvc" {
                    targetdir.join(&format!("{}.dll.lib", lib_name))
                } else {
                    targetdir.join(&format!("{}.dll.a", lib_name))
                };
                let def = targetdir.join(&format!("{}.def", lib_name));
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

        BuildTargets {
            pc,
            include,
            static_lib,
            shared_lib,
            impl_lib,
            def,
            target: target.clone(),
        }
    }
}
