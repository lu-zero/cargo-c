use clap::ArgMatches;
use std::path::{Component, Path, PathBuf};

use cargo::core::Workspace;
use cargo_util::paths::{copy, create_dir_all};

use crate::build::*;
use crate::build_targets::BuildTargets;

fn append_to_destdir(destdir: Option<&Path>, path: &Path) -> PathBuf {
    if let Some(destdir) = destdir {
        let mut joined = destdir.to_path_buf();
        for component in path.components() {
            match component {
                Component::Prefix(_) | Component::RootDir => {}
                _ => joined.push(component),
            };
        }
        joined
    } else {
        path.to_path_buf()
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    #[test]
    fn append_to_destdir() {
        assert_eq!(
            super::append_to_destdir(Some(Path::new(r"/foo")), &PathBuf::from(r"/bar/./..")),
            PathBuf::from(r"/foo/bar/./..")
        );

        assert_eq!(
            super::append_to_destdir(Some(Path::new(r"foo")), &PathBuf::from(r"bar")),
            PathBuf::from(r"foo/bar")
        );

        assert_eq!(
            super::append_to_destdir(Some(Path::new(r"")), &PathBuf::from(r"")),
            PathBuf::from(r"")
        );

        if cfg!(windows) {
            assert_eq!(
                super::append_to_destdir(Some(Path::new(r"X:\foo")), &PathBuf::from(r"Y:\bar")),
                PathBuf::from(r"X:\foo\bar")
            );

            assert_eq!(
                super::append_to_destdir(Some(Path::new(r"A:\foo")), &PathBuf::from(r"B:bar")),
                PathBuf::from(r"A:\foo\bar")
            );

            assert_eq!(
                super::append_to_destdir(Some(Path::new(r"\foo")), &PathBuf::from(r"\bar")),
                PathBuf::from(r"\foo\bar")
            );

            assert_eq!(
                super::append_to_destdir(
                    Some(Path::new(r"C:\dest")),
                    Path::new(r"\\server\share\foo\bar")
                ),
                PathBuf::from(r"C:\\dest\\foo\\bar")
            );
        }
    }
}

pub(crate) enum LibType {
    So,
    Dylib,
    Windows,
}

impl LibType {
    pub(crate) fn from_build_targets(build_targets: &BuildTargets) -> Self {
        let target = &build_targets.target;
        let os = &target.os;
        let env = &target.env;

        match (os.as_str(), env.as_str()) {
            ("linux", _)
            | ("freebsd", _)
            | ("dragonfly", _)
            | ("netbsd", _)
            | ("android", _)
            | ("haiku", _)
            | ("illumos", _)
            | ("emscripten", _) => LibType::So,
            ("macos", _) | ("ios", _) | ("tvos", _) => LibType::Dylib,
            ("windows", _) => LibType::Windows,
            _ => unimplemented!("The target {}-{} is not supported yet", os, env),
        }
    }
}

pub(crate) struct UnixLibNames {
    canonical: String,
    with_main_ver: String,
    with_full_ver: String,
}

impl UnixLibNames {
    pub(crate) fn new(lib_type: LibType, library: &LibraryCApiConfig) -> Option<Self> {
        let lib_name = &library.name;
        let lib_version = &library.version;
        let main_version = library.sover();

        match lib_type {
            LibType::So => {
                let lib = format!("lib{lib_name}.so");
                let lib_with_full_ver = format!(
                    "{}.{}.{}.{}",
                    lib, lib_version.major, lib_version.minor, lib_version.patch
                );
                let lib_with_main_ver = format!("{}.{}", lib, main_version);

                Some(Self {
                    canonical: lib,
                    with_main_ver: lib_with_main_ver,
                    with_full_ver: lib_with_full_ver,
                })
            }
            LibType::Dylib => {
                let lib = format!("lib{lib_name}.dylib");
                let lib_with_main_ver = format!("lib{}.{}.dylib", lib_name, main_version);

                let lib_with_full_ver = format!(
                    "lib{}.{}.{}.{}.dylib",
                    lib_name, lib_version.major, lib_version.minor, lib_version.patch
                );
                Some(Self {
                    canonical: lib,
                    with_main_ver: lib_with_main_ver,
                    with_full_ver: lib_with_full_ver,
                })
            }
            LibType::Windows => None,
        }
    }

    fn links(&self, install_path_lib: &Path) {
        if self.with_main_ver != self.with_full_ver {
            let mut ln_sf = std::process::Command::new("ln");
            ln_sf.arg("-sf");
            ln_sf
                .arg(&self.with_full_ver)
                .arg(install_path_lib.join(&self.with_main_ver));
            let _ = ln_sf.status().unwrap();
        }

        let mut ln_sf = std::process::Command::new("ln");
        ln_sf.arg("-sf");
        ln_sf
            .arg(&self.with_full_ver)
            .arg(install_path_lib.join(&self.canonical));
        let _ = ln_sf.status().unwrap();
    }

    pub(crate) fn install(
        &self,
        capi_config: &CApiConfig,
        shared_lib: &Path,
        install_path_lib: &Path,
    ) -> anyhow::Result<()> {
        if capi_config.library.versioning {
            copy(shared_lib, install_path_lib.join(&self.with_full_ver))?;
            self.links(install_path_lib);
        } else {
            copy(shared_lib, install_path_lib.join(&self.canonical))?;
        }
        Ok(())
    }
}

pub fn cinstall(ws: &Workspace, packages: &[CPackage]) -> anyhow::Result<()> {
    for pkg in packages {
        let paths = &pkg.install_paths;
        let capi_config = &pkg.capi_config;
        let build_targets = &pkg.build_targets;

        let destdir = &paths.destdir;

        let mut install_path_lib = paths.libdir.clone();
        if let Some(subdir) = &capi_config.library.install_subdir {
            install_path_lib.push(subdir);
        }

        let install_path_lib = append_to_destdir(destdir.as_deref(), &install_path_lib);
        let install_path_pc = append_to_destdir(destdir.as_deref(), &paths.pkgconfigdir);
        let install_path_include = append_to_destdir(destdir.as_deref(), &paths.includedir);
        let install_path_data = append_to_destdir(destdir.as_deref(), &paths.datadir);

        create_dir_all(&install_path_lib)?;
        create_dir_all(&install_path_pc)?;

        ws.config()
            .shell()
            .status("Installing", "pkg-config file")?;

        copy(
            &build_targets.pc,
            install_path_pc.join(build_targets.pc.file_name().unwrap()),
        )?;

        if capi_config.header.enabled {
            ws.config().shell().status("Installing", "header file")?;
            for (from, to) in build_targets.extra.include.iter() {
                let to = install_path_include.join(to);
                create_dir_all(to.parent().unwrap())?;
                copy(from, to)?;
            }
        }

        if !build_targets.extra.data.is_empty() {
            ws.config().shell().status("Installing", "data file")?;
            for (from, to) in build_targets.extra.data.iter() {
                let to = install_path_data.join(to);
                create_dir_all(to.parent().unwrap())?;
                copy(from, to)?;
            }
        }

        if let Some(ref static_lib) = build_targets.static_lib {
            ws.config().shell().status("Installing", "static library")?;
            copy(
                static_lib,
                install_path_lib.join(static_lib.file_name().unwrap()),
            )?;
        }

        if let Some(ref shared_lib) = build_targets.shared_lib {
            ws.config().shell().status("Installing", "shared library")?;

            let lib_type = LibType::from_build_targets(build_targets);
            match lib_type {
                LibType::So | LibType::Dylib => {
                    let lib = UnixLibNames::new(lib_type, &capi_config.library).unwrap();
                    lib.install(capi_config, shared_lib, &install_path_lib)?;
                }
                LibType::Windows => {
                    let lib_name = shared_lib.file_name().unwrap();

                    if capi_config.library.install_subdir.is_none() {
                        let install_path_bin = append_to_destdir(destdir.as_deref(), &paths.bindir);
                        create_dir_all(&install_path_bin)?;

                        copy(shared_lib, install_path_bin.join(lib_name))?;
                    } else {
                        // We assume they are plugins, install them in the custom libdir path
                        copy(shared_lib, install_path_lib.join(lib_name))?;
                    }
                    if capi_config.library.import_library {
                        let impl_lib = build_targets.impl_lib.as_ref().unwrap();
                        let impl_lib_name = impl_lib.file_name().unwrap();
                        copy(impl_lib, install_path_lib.join(impl_lib_name))?;
                        let def = build_targets.def.as_ref().unwrap();
                        let def_name = def.file_name().unwrap();
                        copy(def, install_path_lib.join(def_name))?;
                    }
                }
            }
        }
    }

    Ok(())
}

#[derive(Debug, Hash, Clone)]
pub struct InstallPaths {
    pub subdir_name: PathBuf,
    pub destdir: Option<PathBuf>,
    pub prefix: PathBuf,
    pub libdir: PathBuf,
    pub includedir: PathBuf,
    pub datadir: PathBuf,
    pub bindir: PathBuf,
    pub pkgconfigdir: PathBuf,
}

impl InstallPaths {
    pub fn new(_name: &str, args: &ArgMatches, capi_config: &CApiConfig) -> Self {
        let destdir = args.get_one::<PathBuf>("destdir").map(PathBuf::from);
        let prefix = args
            .get_one::<PathBuf>("prefix")
            .map(PathBuf::from)
            .unwrap_or_else(|| "/usr/local".into());
        let libdir = prefix.join(args.get_one::<PathBuf>("libdir").unwrap());
        let includedir = prefix.join(args.get_one::<PathBuf>("includedir").unwrap());
        let datarootdir = prefix.join(args.get_one::<PathBuf>("datarootdir").unwrap());
        let datadir = args
            .get_one::<PathBuf>("datadir")
            .map(|d| prefix.join(d))
            .unwrap_or_else(|| datarootdir.clone());

        let subdir_name = PathBuf::from(&capi_config.header.subdirectory);

        let bindir = prefix.join(args.get_one::<PathBuf>("bindir").unwrap());
        let pkgconfigdir = args
            .get_one::<PathBuf>("pkgconfigdir")
            .map(|d| prefix.join(d))
            .unwrap_or_else(|| libdir.join("pkgconfig"));

        InstallPaths {
            subdir_name,
            destdir,
            prefix,
            libdir,
            includedir,
            datadir,
            bindir,
            pkgconfigdir,
        }
    }
}
