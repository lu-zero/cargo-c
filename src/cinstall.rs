use std::path::PathBuf;

use cargo::core::Workspace;
use cargo::util::command_prelude::opt;
use cargo::util::command_prelude::{AppExt, ArgMatchesExt};
use cargo::CliResult;
use cargo::Config;

use cargo_c::build::{cbuild, config_configure, Common};
use cargo_c::build_targets::BuildTargets;
use cargo_c::install_paths::InstallPaths;
use cargo_c::target::Target;

use structopt::clap::*;
use structopt::StructOpt;

pub fn cli() -> App<'static, 'static> {
    let subcommand = Common::clap()
        .name("cinstall")
        .arg(opt("quiet", "No output printed to stdout").short("q"))
        .arg_package_spec(
            "Package to build (see `cargo help pkgid`)",
            "Build all packages in the workspace",
            "Exclude packages from the build",
        )
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
    let path = if path.is_absolute() {
        let mut components = path.components();
        let _ = components.next();
        components.as_path()
    } else {
        path.as_path()
    };

    destdir.join(path)
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

    let name = pkg
        .manifest()
        .targets()
        .iter()
        .find(|t| t.is_lib())
        .unwrap()
        .name();

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
    ws.config().shell().status("Installing", "static library")?;
    fs::copy(
        &build_targets.static_lib,
        install_path_lib.join(&format!("lib{}.a", name)),
    )?;

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
            let lib_with_full_ver = &format!("{}.{}.{}", lib_with_major_ver, ver.minor, ver.patch);
            fs::copy(
                &build_targets.shared_lib,
                install_path_lib.join(lib_with_full_ver),
            )?;
            link_libs(lib, lib_with_major_ver, lib_with_full_ver);
        }
        ("macos", _) => {
            let lib = &format!("lib{}.dylib", name);
            let lib_with_major_ver = &format!("lib{}.{}.dylib", name, ver.major);
            let lib_with_full_ver = &format!(
                "lib{}.{}.{}.{}.dylib",
                name, ver.major, ver.minor, ver.patch
            );
            fs::copy(
                &build_targets.shared_lib,
                install_path_lib.join(lib_with_full_ver),
            )?;
            link_libs(lib, lib_with_major_ver, lib_with_full_ver);
        }
        ("windows", "gnu") => {
            let lib = format!("{}.dll", name);
            let impl_lib = format!("lib{}.dll.a", name);
            let def = format!("{}.def", name);
            fs::copy(&build_targets.shared_lib, install_path_bin.join(lib))?;
            fs::copy(
                build_targets.impl_lib.as_ref().unwrap(),
                install_path_lib.join(impl_lib),
            )?;
            fs::copy(
                build_targets.def.as_ref().unwrap(),
                install_path_lib.join(def),
            )?;
        }
        _ => unimplemented!("The target {}-{} is not supported yet", os, env),
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

    config_configure(&mut config, &args, subcommand_args)?;

    let mut ws = subcommand_args.workspace(&config)?;

    let (build_targets, install_paths) = cbuild(&mut ws, &config, &subcommand_args)?;

    cinstall(
        &ws,
        &Target::new(args.target())?,
        build_targets,
        install_paths,
    )?;

    Ok(())
}
