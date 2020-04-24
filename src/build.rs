use std::io::Write;
use std::path::PathBuf;

use cargo::core::profiles::Profiles;
use cargo::core::{TargetKind, Workspace};
use cargo::ops;
use cargo::util::command_prelude::{ArgMatches, ArgMatchesExt, CompileMode, ProfileChecking};
use cargo::{CliResult, Config};

use semver::Version;

use crate::build_targets::BuildTargets;
use crate::install_paths::InstallPaths;
use crate::pkg_config_gen::PkgConfig;
use crate::static_libs::get_static_libs_for_target;
use crate::target;

/// Build the C header
fn build_include_file(
    ws: &Workspace,
    name: &str,
    version: &Version,
    root_output: &PathBuf,
    root_path: &PathBuf,
) -> anyhow::Result<()> {
    ws.config()
        .shell()
        .status("Building", "header file using cbindgen")?;
    let include_path = root_output.join(&format!("{}.h", name));
    let crate_path = root_path;

    // TODO: map the errors
    let mut config = cbindgen::Config::from_root_or_default(crate_path);
    let warning = config.autogen_warning.unwrap_or_default();
    let version_info = format!(
        "\n#define {0}_MAJOR {1}\n#define {0}_MINOR {2}\n#define {0}_PATCH {3}\n",
        name.to_uppercase(),
        version.major,
        version.minor,
        version.patch
    );
    config.autogen_warning = Some(warning + &version_info);
    cbindgen::Builder::new()
        .with_crate(crate_path)
        .with_config(config)
        .generate()
        .unwrap()
        .write_to_file(include_path);

    Ok(())
}

fn build_pc_file(
    ws: &Workspace,
    name: &str,
    root_output: &PathBuf,
    pc: &PkgConfig,
) -> anyhow::Result<()> {
    ws.config().shell().status("Building", "pkg-config file")?;
    let pc_path = root_output.join(&format!("{}.pc", name));

    let mut out = std::fs::File::create(pc_path)?;

    let buf = pc.render();

    out.write_all(buf.as_ref())?;

    Ok(())
}

fn patch_lib_kind_in_target(ws: &mut Workspace, libkinds: &[&str]) -> anyhow::Result<()> {
    use cargo::core::LibKind::*;

    let pkg = ws.current_mut()?;
    let manifest = pkg.manifest_mut();
    let targets = manifest.targets_mut();

    let kinds: Vec<_> = libkinds.iter().map(|&kind| Other(kind.into())).collect();

    for target in targets.iter_mut() {
        if target.is_lib() {
            *target.kind_mut() = TargetKind::Lib(kinds.clone());
        }
    }

    Ok(())
}

/// Build import library for Windows
fn build_implib_file(
    ws: &Workspace,
    name: &str,
    target: &target::Target,
    targetdir: &PathBuf,
) -> anyhow::Result<()> {
    let os = &target.os;
    let env = &target.env;

    if os == "windows" && env == "gnu" {
        ws.config()
            .shell()
            .status("Building", "implib using dlltool")?;

        let arch = &target.arch;

        let binutils_arch = match arch.as_str() {
            "x86_64" => "i386:x86-64",
            "x86" => "i386",
            _ => unimplemented!("Windows support for {} is not implemented yet.", arch),
        };

        let mut dlltool = std::process::Command::new("dlltool");
        dlltool.arg("-m").arg(binutils_arch);
        dlltool.arg("-D").arg(format!("{}.dll", name));
        dlltool
            .arg("-l")
            .arg(targetdir.join(format!("{}.dll.a", name)));
        dlltool
            .arg("-d")
            .arg(targetdir.join(format!("{}.def", name)));

        let out = dlltool.output()?;
        if out.status.success() {
            Ok(())
        } else {
            Err(anyhow::anyhow!("Command failed {:?}", dlltool))
        }
    } else {
        Ok(())
    }
}

fn fingerprint(build_targets: &BuildTargets) -> anyhow::Result<Option<u64>> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::Hasher;
    use std::io::Read;

    let mut hasher = DefaultHasher::new();

    let mut paths = vec![&build_targets.include];
    build_targets.static_lib.as_ref().map(|p| paths.push(p));
    build_targets.shared_lib.as_ref().map(|p| paths.push(p));

    for path in paths.iter() {
        if let Ok(mut f) = std::fs::File::open(path) {
            let mut buf = Vec::new();
            f.read_to_end(&mut buf)?;

            hasher.write(&buf);
        } else {
            return Ok(None);
        };
    }

    Ok(Some(hasher.finish()))
}

pub fn cbuild(
    ws: &mut Workspace,
    config: &Config,
    args: &ArgMatches<'_>,
) -> anyhow::Result<(BuildTargets, InstallPaths)> {
    let rustc_target = target::Target::new(args.target())?;
    let install_paths = InstallPaths::from_matches(args);
    let libkinds = args
        .values_of("library-type")
        .map_or_else(|| vec!["staticlib", "cdylib"], |v| v.collect::<Vec<_>>());
    let only_staticlib = !libkinds.contains(&"cdylib");

    patch_lib_kind_in_target(ws, &libkinds)?;

    let name = &ws
        .current()?
        .manifest()
        .targets()
        .iter()
        .find(|t| t.is_lib())
        .unwrap()
        .crate_name();
    let version = ws.current()?.version().clone();
    let root_path = ws.current()?.root().to_path_buf();

    let mut pc = PkgConfig::from_workspace(name, ws, &install_paths, args);

    let static_libs = get_static_libs_for_target(
        rustc_target.verbatim.as_ref(),
        &ws.target_dir().as_path_unlocked().to_path_buf(),
    )?;

    if only_staticlib {
        pc.add_lib(&static_libs);
    }
    pc.add_lib_private(&static_libs);

    let mut compile_opts = args.compile_options(
        config,
        CompileMode::Build,
        Some(ws),
        ProfileChecking::Checked,
    )?;

    compile_opts.filter = ops::CompileFilter::new(
        ops::LibRule::True,
        ops::FilterRule::none(),
        ops::FilterRule::none(),
        ops::FilterRule::none(),
        ops::FilterRule::none(),
    );

    compile_opts.export_dir = args.value_of_path("out-dir", config);
    if compile_opts.export_dir.is_some() {
        config
            .cli_unstable()
            .fail_if_stable_opt("--out-dir", 6790)?;
    }

    let profiles = Profiles::new(
        ws.profiles(),
        config,
        compile_opts.build_config.requested_profile,
        ws.features(),
    )?;

    // TODO: there must be a simpler way to get the right path.
    let root_output = ws
        .target_dir()
        .as_path_unlocked()
        .to_path_buf()
        .join(&profiles.get_dir_name());

    let mut link_args: Vec<String> = rustc_target
        .shared_object_link_args(name, ws, &install_paths.libdir, &root_output)
        .into_iter()
        .flat_map(|l| vec!["-C".to_string(), format!("link-arg={}", l)])
        .collect();

    link_args.push("--cfg".into());
    link_args.push("cargo_c".into());

    compile_opts.target_rustc_args = Some(link_args);

    let build_targets = BuildTargets::new(&name, &rustc_target, &root_output, &libkinds);

    let prev_hash = fingerprint(&build_targets)?;

    let r = ops::compile(ws, &compile_opts)?;
    assert_eq!(root_output, r.root_output);

    let cur_hash = fingerprint(&build_targets)?;

    build_pc_file(&ws, &name, &root_output, &pc)?;

    if cur_hash.is_none() || prev_hash != cur_hash {
        build_implib_file(&ws, &name, &rustc_target, &root_output)?;

        build_include_file(&ws, &name, &version, &root_output, &root_path)?;
    }

    Ok((build_targets, install_paths))
}

pub fn config_configure(config: &mut Config, args: &ArgMatches<'_>) -> CliResult {
    let arg_target_dir = &args.value_of_path("target-dir", config);
    let config_args: Vec<_> = args
        .values_of("config")
        .unwrap_or_default()
        .map(String::from)
        .collect();
    config.configure(
        args.occurrences_of("verbose") as u32,
        args.is_present("quiet"),
        args.value_of("color"),
        args.is_present("frozen"),
        args.is_present("locked"),
        args.is_present("offline"),
        arg_target_dir,
        &args
            .values_of_lossy("unstable-features")
            .unwrap_or_default(),
        &config_args,
    )?;
    Ok(())
}
