use std::path::{Component, Path, PathBuf};
use structopt::clap::ArgMatches;

use cargo::core::Workspace;

use crate::build::CApiConfig;
use crate::build_targets::BuildTargets;

use anyhow::Context;

fn append_to_destdir(destdir: &PathBuf, path: &PathBuf) -> PathBuf {
    let mut joined = destdir.clone();
    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir => {}
            _ => joined.push(component),
        };
    }
    joined
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    #[test]
    fn append_to_destdir() {
        assert_eq!(
            super::append_to_destdir(&PathBuf::from(r"/foo"), &PathBuf::from(r"/bar/./..")),
            PathBuf::from(r"/foo/bar/./..")
        );

        assert_eq!(
            super::append_to_destdir(&PathBuf::from(r"foo"), &PathBuf::from(r"bar")),
            PathBuf::from(r"foo/bar")
        );

        assert_eq!(
            super::append_to_destdir(&PathBuf::from(r""), &PathBuf::from(r"")),
            PathBuf::from(r"")
        );

        if cfg!(windows) {
            assert_eq!(
                super::append_to_destdir(&PathBuf::from(r"X:\foo"), &PathBuf::from(r"Y:\bar")),
                PathBuf::from(r"X:\foo\bar")
            );

            assert_eq!(
                super::append_to_destdir(&PathBuf::from(r"A:\foo"), &PathBuf::from(r"B:bar")),
                PathBuf::from(r"A:\foo\bar")
            );

            assert_eq!(
                super::append_to_destdir(&PathBuf::from(r"\foo"), &PathBuf::from(r"\bar")),
                PathBuf::from(r"\foo\bar")
            );

            assert_eq!(
                super::append_to_destdir(
                    &PathBuf::from(r"C:\dest"),
                    &PathBuf::from(r"\\server\share\foo\bar")
                ),
                PathBuf::from(r"C:\\dest\\foo\\bar")
            );
        }
    }
}

fn copy<P: AsRef<Path>, Q: AsRef<Path>>(from: P, to: Q) -> anyhow::Result<u64> {
    let from = from.as_ref();
    let to = to.as_ref();
    std::fs::copy(from, to)
        .with_context(|| format!("Cannot copy {} to {}.", from.display(), to.display()))
}

pub fn cinstall(
    ws: &Workspace,
    capi_config: &CApiConfig,
    build_targets: BuildTargets,
    paths: InstallPaths,
) -> anyhow::Result<()> {
    use std::fs;

    let target = &build_targets.target;
    let os = &target.os;
    let env = &target.env;

    let destdir = &paths.destdir;

    let mut install_path_lib = paths.libdir.clone();
    if let Some(subdir) = &capi_config.library.install_subdir {
        install_path_lib.push(subdir);
    }

    let install_path_lib = append_to_destdir(destdir, &install_path_lib);
    let install_path_pc = append_to_destdir(destdir, &paths.pkgconfigdir);
    let mut install_path_include = append_to_destdir(destdir, &paths.includedir);
    if let Some(name) = paths.subdir_name {
        install_path_include = install_path_include.join(name);
    }

    fs::create_dir_all(&install_path_lib)?;
    fs::create_dir_all(&install_path_pc)?;

    ws.config()
        .shell()
        .status("Installing", "pkg-config file")?;
    fs::copy(
        &build_targets.pc,
        install_path_pc.join(build_targets.pc.file_name().unwrap()),
    )?;

    if capi_config.header.enabled {
        fs::create_dir_all(&install_path_include)?;
        ws.config().shell().status("Installing", "header file")?;
        let include = &build_targets.include.unwrap();
        fs::copy(
            include,
            install_path_include.join(include.file_name().unwrap()),
        )?;
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

        let lib_name = &capi_config.library.name;
        let lib_version = &capi_config.library.version;

        let link_libs = |lib: &str, lib_with_major_ver: &str, lib_with_full_ver: &str| {
            let mut ln_sf = std::process::Command::new("ln");
            ln_sf.arg("-sf");
            ln_sf
                .arg(lib_with_full_ver)
                .arg(install_path_lib.join(lib_with_major_ver));
            let _ = ln_sf.status().unwrap();
            let mut ln_sf = std::process::Command::new("ln");
            ln_sf.arg("-sf");
            ln_sf.arg(lib_with_full_ver).arg(install_path_lib.join(lib));
            let _ = ln_sf.status().unwrap();
        };

        match (os.as_str(), env.as_str()) {
            ("linux", _) | ("freebsd", _) | ("dragonfly", _) | ("netbsd", _) => {
                let lib = &format!("lib{}.so", lib_name);
                let lib_with_major_ver = &format!("{}.{}", lib, lib_version.major);
                let lib_with_full_ver = &format!(
                    "{}.{}.{}",
                    lib_with_major_ver, lib_version.minor, lib_version.patch
                );
                if capi_config.library.versioning {
                    copy(shared_lib, install_path_lib.join(lib_with_full_ver))?;
                    link_libs(lib, lib_with_major_ver, lib_with_full_ver);
                } else {
                    copy(shared_lib, install_path_lib.join(lib))?;
                }
            }
            ("macos", _) => {
                let lib = &format!("lib{}.dylib", lib_name);
                let lib_with_major_ver = &format!("lib{}.{}.dylib", lib_name, lib_version.major);
                let lib_with_full_ver = &format!(
                    "lib{}.{}.{}.{}.dylib",
                    lib_name, lib_version.major, lib_version.minor, lib_version.patch
                );
                if capi_config.library.versioning {
                    copy(shared_lib, install_path_lib.join(lib_with_full_ver))?;
                    link_libs(lib, lib_with_major_ver, lib_with_full_ver);
                } else {
                    copy(shared_lib, install_path_lib.join(lib))?;
                }
            }
            ("windows", _) => {
                let install_path_bin = append_to_destdir(destdir, &paths.bindir);
                fs::create_dir_all(&install_path_bin)?;

                let lib_name = shared_lib.file_name().unwrap();
                copy(shared_lib, install_path_bin.join(lib_name))?;
                let impl_lib = build_targets.impl_lib.as_ref().unwrap();
                let impl_lib_name = impl_lib.file_name().unwrap();
                copy(impl_lib, install_path_lib.join(impl_lib_name))?;
                let def = build_targets.def.as_ref().unwrap();
                let def_name = def.file_name().unwrap();
                copy(def, install_path_lib.join(def_name))?;
            }
            _ => unimplemented!("The target {}-{} is not supported yet", os, env),
        }
    }

    Ok(())
}

#[derive(Debug)]
pub struct InstallPaths {
    pub subdir_name: Option<PathBuf>,
    pub destdir: PathBuf,
    pub prefix: PathBuf,
    pub libdir: PathBuf,
    pub includedir: PathBuf,
    pub bindir: PathBuf,
    pub pkgconfigdir: PathBuf,
}

impl InstallPaths {
    pub fn new(name: &str, args: &ArgMatches<'_>, capi_config: &CApiConfig) -> Self {
        let destdir = args
            .value_of("destdir")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/"));
        let prefix = args
            .value_of("prefix")
            .map(PathBuf::from)
            .unwrap_or_else(|| "/usr/local".into());
        let libdir = args
            .value_of("libdir")
            .map(PathBuf::from)
            .unwrap_or_else(|| prefix.join("lib"));
        let includedir = args
            .value_of("includedir")
            .map(PathBuf::from)
            .unwrap_or_else(|| prefix.join("include"));
        let subdir_name = if capi_config.header.subdirectory {
            Some(PathBuf::from(name))
        } else {
            None
        };
        let bindir = args
            .value_of("bindir")
            .map(PathBuf::from)
            .unwrap_or_else(|| prefix.join("bin"));
        let pkgconfigdir = args
            .value_of("pkgconfigdir")
            .map(PathBuf::from)
            .unwrap_or_else(|| libdir.join("pkgconfig"));

        InstallPaths {
            subdir_name,
            destdir,
            prefix,
            libdir,
            includedir,
            bindir,
            pkgconfigdir,
        }
    }
}
