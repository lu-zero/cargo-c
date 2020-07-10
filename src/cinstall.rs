use std::path::{Component, Path, PathBuf};

use cargo::core::Workspace;
use cargo::util::command_prelude::opt;
use cargo::util::command_prelude::{AppExt, ArgMatchesExt};
use cargo::CliResult;
use cargo::Config;

use cargo_c::build::{cbuild, config_configure};
use cargo_c::build_targets::BuildTargets;
use cargo_c::cli::base_cli;
use cargo_c::install_paths::InstallPaths;
use cargo_c::target::Target;

use anyhow::Context;
use structopt::clap::*;

pub fn cli() -> App<'static, 'static> {
    let subcommand = base_cli()
        .name("cinstall")
        .arg_jobs()
        .arg_release("Build artifacts in release mode, with optimizations")
        .arg_profile("Build artifacts with the specified profile")
        .arg_features()
        .arg_target_triple("Build for the target triple")
        .arg_target_dir()
        .arg(
            opt(
                "out-dir",
                "Copy final artifacts to this directory (unstable)",
            )
            .value_name("PATH"),
        )
        .arg_manifest_path()
        .arg_message_format()
        .arg_build_plan()
        .after_help(
            "\
Compilation can be configured via the use of profiles which are configured in
the manifest. The default profile for this command is `dev`, but passing
the --release flag will use the `release` profile instead.
",
        );

    app_from_crate!()
        .settings(&[
            AppSettings::UnifiedHelpMessage,
            AppSettings::DeriveDisplayOrder,
            AppSettings::VersionlessSubcommands,
        ])
        .subcommand(subcommand)
}

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

fn cinstall(
    ws: &Workspace,
    target: &Target,
    build_targets: BuildTargets,
    paths: InstallPaths,
) -> anyhow::Result<()> {
    use std::fs;

    let pkg = ws.current()?;

    let os = &target.os;
    let env = &target.env;
    let ver = pkg.version();

    let name = &pkg
        .manifest()
        .targets()
        .iter()
        .find(|t| t.is_lib())
        .unwrap()
        .crate_name();

    let destdir = &paths.destdir;

    let install_path_lib = append_to_destdir(destdir, &paths.libdir);
    let install_path_pc = append_to_destdir(destdir, &paths.pkgconfigdir);
    let install_path_include = append_to_destdir(destdir, &paths.includedir).join(name);
    let install_path_bin = append_to_destdir(destdir, &paths.bindir);

    fs::create_dir_all(&install_path_lib)?;
    fs::create_dir_all(&install_path_pc)?;
    fs::create_dir_all(&install_path_include)?;
    fs::create_dir_all(&install_path_bin)?;

    ws.config()
        .shell()
        .status("Installing", "pkg-config file")?;
    fs::copy(
        &build_targets.pc,
        install_path_pc.join(&format!("{}.pc", name)),
    )?;
    ws.config().shell().status("Installing", "header file")?;
    fs::copy(
        &build_targets.include,
        install_path_include.join(&format!("{}.h", name)),
    )?;

    if let Some(ref static_lib) = build_targets.static_lib {
        ws.config().shell().status("Installing", "static library")?;
        let static_lib_path = if env == "msvc" {
            format!("{}.lib", name)
        } else {
            format!("lib{}.a", name)
        };
        copy(static_lib, install_path_lib.join(&static_lib_path))?;
    }

    if let Some(ref shared_lib) = build_targets.shared_lib {
        ws.config().shell().status("Installing", "shared library")?;
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
                let lib = &format!("lib{}.so", name);
                let lib_with_major_ver = &format!("{}.{}", lib, ver.major);
                let lib_with_full_ver =
                    &format!("{}.{}.{}", lib_with_major_ver, ver.minor, ver.patch);
                copy(shared_lib, install_path_lib.join(lib_with_full_ver))?;
                link_libs(lib, lib_with_major_ver, lib_with_full_ver);
            }
            ("macos", _) => {
                let lib = &format!("lib{}.dylib", name);
                let lib_with_major_ver = &format!("lib{}.{}.dylib", name, ver.major);
                let lib_with_full_ver = &format!(
                    "lib{}.{}.{}.{}.dylib",
                    name, ver.major, ver.minor, ver.patch
                );
                copy(shared_lib, install_path_lib.join(lib_with_full_ver))?;
                link_libs(lib, lib_with_major_ver, lib_with_full_ver);
            }
            ("windows", ref env) => {
                let lib = format!("{}.dll", name);
                let impl_lib = if *env == "msvc" {
                    format!("{}.dll.lib", name)
                } else {
                    format!("lib{}.dll.a", name)
                };
                let def = format!("{}.def", name);
                copy(shared_lib, install_path_bin.join(lib))?;
                copy(
                    build_targets.impl_lib.as_ref().unwrap(),
                    install_path_lib.join(impl_lib),
                )?;
                copy(
                    build_targets.def.as_ref().unwrap(),
                    install_path_lib.join(def),
                )?;
            }
            _ => unimplemented!("The target {}-{} is not supported yet", os, env),
        }
    }

    Ok(())
}

fn main() -> CliResult {
    let mut config = Config::default()?;

    let args = cli().get_matches();

    let subcommand_args = match args.subcommand() {
        ("cinstall", Some(args)) => args,
        _ => {
            // No subcommand provided.
            cli().print_help()?;
            return Ok(());
        }
    };

    config_configure(&mut config, subcommand_args)?;

    let mut ws = subcommand_args.workspace(&config)?;

    let (build_targets, install_paths) = cbuild(&mut ws, &config, &subcommand_args)?;

    cinstall(
        &ws,
        &Target::new(subcommand_args.target())?,
        build_targets,
        install_paths,
    )?;

    Ok(())
}
