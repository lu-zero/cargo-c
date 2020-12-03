use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;

use cargo::core::profiles::Profiles;
use cargo::core::{TargetKind, Workspace};
use cargo::ops::{self, CompileFilter, CompileOptions, FilterRule, LibRule};
use cargo::util::command_prelude::{ArgMatches, ArgMatchesExt, CompileMode, ProfileChecking};
use cargo::util::errors;
use cargo::{CliError, CliResult, Config};

use anyhow::Error;
use semver::Version;

use crate::build_targets::BuildTargets;
use crate::install::InstallPaths;
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
    let mut header_name = PathBuf::from(name);
    header_name.set_extension("h");
    let include_path = root_output.join(header_name);
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

/// Copy the pre-built C header from the asset directory
fn copy_prebuilt_include_file(
    ws: &Workspace,
    name: &str,
    root_output: &PathBuf,
    root_path: &PathBuf,
) -> anyhow::Result<()> {
    ws.config()
        .shell()
        .status("Building", "pre-built header file")?;
    let mut header_name = PathBuf::from(name);
    header_name.set_extension("h");

    let source_path = root_path.join("assets").join(&header_name);
    let target_path = root_output.join(header_name);

    std::fs::copy(source_path, target_path)?;

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
    use cargo::core::compiler::CrateType;

    let pkg = ws.current_mut()?;
    let manifest = pkg.manifest_mut();
    let targets = manifest.targets_mut();

    let mut kinds: Vec<_> = libkinds
        .iter()
        .map(|&kind| match kind {
            "staticlib" => CrateType::Staticlib,
            "cdylib" => CrateType::Cdylib,
            _ => unreachable!(),
        })
        .collect();

    kinds.push(CrateType::Lib);

    for target in targets.iter_mut() {
        if target.is_lib() {
            target.set_kind(TargetKind::Lib(kinds.clone()));
        }
    }

    Ok(())
}

fn patch_capi_feature(compile_opts: &mut CompileOptions, ws: &Workspace) -> anyhow::Result<()> {
    let pkg = ws.current()?;
    let manifest = pkg.manifest();

    if manifest.summary().features().get("capi").is_some() {
        compile_opts.features.push("capi".to_string());
    }

    Ok(())
}

/// Build def file for windows-msvc
fn build_def_file(
    ws: &Workspace,
    name: &str,
    target: &target::Target,
    targetdir: &PathBuf,
) -> anyhow::Result<()> {
    let os = &target.os;
    let env = &target.env;

    if os == "windows" && env == "msvc" {
        ws.config()
            .shell()
            .status("Building", ".def file using dumpbin")?;

        let txt_path = targetdir.join(format!("{}.txt", name));
        let mut dumpbin = std::process::Command::new("dumpbin");
        dumpbin
            .arg("/EXPORTS")
            .arg(targetdir.join(format!("{}.dll", name)));
        dumpbin.arg(format!("/OUT:{}", txt_path.to_str().unwrap()));

        let out = dumpbin.output()?;
        if out.status.success() {
            let txt_file = File::open(txt_path)?;
            let buf_reader = BufReader::new(txt_file);
            let mut def_file = File::create(targetdir.join(format!("{}.def", name)))?;
            writeln!(def_file, "{}", "EXPORTS".to_string())?;

            // The Rust loop below is analogue to the following loop.
            // for /f "skip=19 tokens=4" %A in (file.txt) do echo %A > file.def
            // The most recent versions of dumpbin adds three lines of copyright
            // information before the relevant content.
            // If the "/OUT:file.txt" dumpbin's option is used, the three
            // copyright lines are added to the shell, so the txt file
            // contains three lines less.
            // The Rust loop first skips 16 lines and then, for each line,
            // deletes all the characters up to the fourth space included
            // (skip=16 tokens=4)
            for line in buf_reader
                .lines()
                .skip(16)
                .take_while(|l| !l.as_ref().unwrap().is_empty())
                .map(|l| {
                    l.unwrap()
                        .as_str()
                        .split_whitespace()
                        .nth(3)
                        .unwrap()
                        .to_string()
                })
            {
                writeln!(def_file, "\t{}", line)?;
            }

            Ok(())
        } else {
            Err(anyhow::anyhow!("Command failed {:?}", dumpbin))
        }
    } else {
        Ok(())
    }
}

/// Build import library for windows-gnu
fn build_implib_file(
    ws: &Workspace,
    name: &str,
    target: &target::Target,
    targetdir: &PathBuf,
    dlltool: &PathBuf,
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

        let mut dlltool_command = std::process::Command::new(dlltool.to_str().unwrap_or("dlltool"));
        dlltool_command.arg("-m").arg(binutils_arch);
        dlltool_command.arg("-D").arg(format!("{}.dll", name));
        dlltool_command
            .arg("-l")
            .arg(targetdir.join(format!("{}.dll.a", name)));
        dlltool_command
            .arg("-d")
            .arg(targetdir.join(format!("{}.def", name)));

        let out = dlltool_command.output()?;
        if out.status.success() {
            Ok(())
        } else {
            Err(anyhow::anyhow!("Command failed {:?}", dlltool_command))
        }
    } else {
        Ok(())
    }
}

struct FingerPrint<'a> {
    name: &'a str,
    root_output: &'a PathBuf,
    build_targets: &'a BuildTargets,
    install_paths: &'a InstallPaths,
}

impl<'a> FingerPrint<'a> {
    fn new(
        name: &'a str,
        root_output: &'a PathBuf,
        build_targets: &'a BuildTargets,
        install_paths: &'a InstallPaths,
    ) -> Self {
        Self {
            name,
            root_output,
            build_targets,
            install_paths,
        }
    }

    fn hash(&self) -> anyhow::Result<Option<u64>> {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        self.install_paths.hash(&mut hasher);

        let mut paths: Vec<&PathBuf> = Vec::new();
        if let Some(include) = &self.build_targets.include {
            paths.push(&include);
        }
        paths.extend(&self.build_targets.static_lib);
        paths.extend(&self.build_targets.shared_lib);

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

    fn path(&self) -> PathBuf {
        // Use the crate name in the cache file as the same target dir
        // may be used to build various libs
        self.root_output
            .join(format!("cargo-c-{}.cache", self.name))
    }

    fn load_previous(&self) -> anyhow::Result<u64> {
        let mut f = std::fs::File::open(&self.path())?;
        let mut buf = [0; 8];
        f.read_exact(&mut buf)?;

        Ok(u64::from_le_bytes(buf))
    }

    fn is_valid(&self) -> bool {
        match (self.load_previous(), self.hash()) {
            (Ok(prev), Ok(Some(current))) => prev == current,
            _ => false,
        }
    }

    fn store(&self) -> anyhow::Result<()> {
        let mut f = std::fs::File::create(&self.path())?;
        if let Some(hash) = self.hash()? {
            f.write_all(&hash.to_le_bytes())?;
        }

        Ok(())
    }
}

pub struct CApiConfig {
    pub header: HeaderCApiConfig,
    pub pkg_config: PkgConfigCApiConfig,
    pub library: LibraryCApiConfig,
}

pub struct HeaderCApiConfig {
    pub name: String,
    pub subdirectory: bool,
    pub generation: bool,
    pub enabled: bool,
}

pub struct PkgConfigCApiConfig {
    pub name: String,
    pub description: String,
    pub version: String,
    pub requires: Option<String>,
    pub requires_private: Option<String>,
}

pub struct LibraryCApiConfig {
    pub name: String,
    pub version: Version,
    pub install_subdir: Option<String>,
    pub versioning: bool,
}

fn load_manifest_capi_config(
    name: &str,
    root_path: &PathBuf,
    ws: &Workspace,
) -> anyhow::Result<CApiConfig> {
    let mut manifest = std::fs::File::open(root_path.join("Cargo.toml"))?;
    let mut manifest_str = String::new();
    manifest.read_to_string(&mut manifest_str)?;

    let toml = manifest_str.parse::<toml::Value>()?;

    let capi = toml
        .get("package")
        .and_then(|v| v.get("metadata"))
        .and_then(|v| v.get("capi"));

    if let Some(min_version) = capi
        .as_ref()
        .and_then(|capi| capi.get("min_version"))
        .and_then(|v| v.as_str())
    {
        let min_version = Version::parse(min_version)?;
        let version = Version::parse(env!("CARGO_PKG_VERSION"))?;
        if min_version > version {
            anyhow::bail!(
                "Minimum required cargo-c version is {} but using cargo-c version {}",
                min_version,
                version
            );
        }
    }

    let header = capi.and_then(|v| v.get("header"));

    let header = if let Some(ref capi) = capi {
        HeaderCApiConfig {
            name: header
                .as_ref()
                .and_then(|h| h.get("name"))
                .or_else(|| capi.get("header_name"))
                .map(|v| v.clone().try_into())
                .unwrap_or_else(|| Ok(String::from(name)))?,
            subdirectory: header
                .as_ref()
                .and_then(|h| h.get("subdirectory"))
                .map(|v| v.clone().try_into())
                .unwrap_or(Ok(true))?,
            generation: header
                .as_ref()
                .and_then(|h| h.get("generation"))
                .map(|v| v.clone().try_into())
                .unwrap_or(Ok(true))?,
            enabled: header
                .as_ref()
                .and_then(|h| h.get("enabled"))
                .map(|v| v.clone().try_into())
                .unwrap_or(Ok(true))?,
        }
    } else {
        HeaderCApiConfig {
            name: String::from(name),
            subdirectory: true,
            generation: true,
            enabled: true,
        }
    };

    let pc = capi.and_then(|v| v.get("pkg_config"));
    let pkg = ws.current().unwrap();
    let mut pc_name = String::from(name);
    let mut description = String::from(
        pkg.manifest()
            .metadata()
            .description
            .as_deref()
            .unwrap_or(""),
    );
    let mut version = pkg.version().to_string();
    let mut requires = None;
    let mut requires_private = None;

    if let Some(ref pc) = pc {
        if let Some(override_name) = pc.get("name").and_then(|v| v.as_str()) {
            pc_name = String::from(override_name);
        }
        if let Some(override_description) = pc.get("description").and_then(|v| v.as_str()) {
            description = String::from(override_description);
        }
        if let Some(override_version) = pc.get("version").and_then(|v| v.as_str()) {
            version = String::from(override_version);
        }
        if let Some(req) = pc.get("requires").and_then(|v| v.as_str()) {
            requires = Some(String::from(req));
        }
        if let Some(req) = pc.get("requires_private").and_then(|v| v.as_str()) {
            requires_private = Some(String::from(req));
        }
    }

    let pkg_config = PkgConfigCApiConfig {
        name: pc_name,
        description,
        version,
        requires,
        requires_private,
    };

    let library = capi.and_then(|v| v.get("library"));
    let mut lib_name = String::from(name);
    let mut version = pkg.version().clone();
    let mut install_subdir = None;
    let mut versioning = true;

    if let Some(ref library) = library {
        if let Some(override_name) = library.get("name").and_then(|v| v.as_str()) {
            lib_name = String::from(override_name);
        }
        if let Some(override_version) = library.get("version").and_then(|v| v.as_str()) {
            version = Version::parse(override_version)?;
        }
        if let Some(subdir) = library.get("install_subdir").and_then(|v| v.as_str()) {
            install_subdir = Some(String::from(subdir));
        }
        versioning = library
            .get("versioning")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
    }

    let library = LibraryCApiConfig {
        name: lib_name,
        version,
        install_subdir,
        versioning,
    };

    Ok(CApiConfig {
        header,
        pkg_config,
        library,
    })
}

pub fn compile_options(
    ws: &Workspace,
    config: &Config,
    args: &ArgMatches<'_>,
    compile_mode: CompileMode,
) -> anyhow::Result<CompileOptions> {
    let mut compile_opts =
        args.compile_options(config, compile_mode, Some(ws), ProfileChecking::Checked)?;

    patch_capi_feature(&mut compile_opts, ws)?;

    compile_opts.filter = CompileFilter::new(
        LibRule::True,
        FilterRule::none(),
        FilterRule::none(),
        FilterRule::none(),
        FilterRule::none(),
    );

    Ok(compile_opts)
}

pub fn cbuild(
    ws: &mut Workspace,
    config: &Config,
    args: &ArgMatches<'_>,
) -> anyhow::Result<(
    BuildTargets,
    InstallPaths,
    CApiConfig,
    String,
    CompileOptions,
)> {
    let targets = args.targets();
    let target = match targets.len() {
        0 => None,
        1 => Some(targets[0].clone()),
        _ => {
            anyhow::bail!("Multiple targets not supported yet");
        }
    };

    let rustc_target = target::Target::new(target.as_ref())?;
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
    let capi_config = load_manifest_capi_config(name, &root_path, &ws)?;

    let install_paths = InstallPaths::new(name, args, &capi_config);

    let mut pc = PkgConfig::from_workspace(name, &install_paths, args, &capi_config);

    let static_libs = get_static_libs_for_target(
        rustc_target.verbatim.as_ref(),
        &ws.target_dir().as_path_unlocked().to_path_buf(),
    )?;

    if only_staticlib {
        pc.add_lib(&static_libs);
    }
    pc.add_lib_private(&static_libs);

    let mut compile_opts = compile_options(ws, config, args, CompileMode::Build)?;

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
        .join(
            target
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(".")),
        )
        .join(&profiles.get_dir_name());

    let mut link_args: Vec<String> = rustc_target
        .shared_object_link_args(&capi_config, &install_paths.libdir, &root_output)
        .into_iter()
        .flat_map(|l| vec!["-C".to_string(), format!("link-arg={}", l)])
        .collect();

    link_args.push("--cfg".into());
    link_args.push("cargo_c".into());

    compile_opts.target_rustc_args = Some(link_args);

    let build_targets =
        BuildTargets::new(&name, &rustc_target, &root_output, &libkinds, &capi_config);

    // TODO: check the root_output
    let _r = ops::compile(ws, &compile_opts)?;

    let finger_print = FingerPrint::new(&name, &root_output, &build_targets, &install_paths);

    if !finger_print.is_valid() {
        build_pc_file(&ws, &name, &root_output, &pc)?;
        let pc_uninstalled = pc.uninstalled(&root_output);
        build_pc_file(
            &ws,
            &format!("{}-uninstalled", name),
            &root_output,
            &pc_uninstalled,
        )?;

        if !only_staticlib {
            build_def_file(&ws, &name, &rustc_target, &root_output)?;

            let mut dlltool = std::env::var_os("DLLTOOL")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("dlltool"));

            // dlltool argument overwrites environment var
            if args.value_of("dlltool").is_some() {
                dlltool = args.value_of("dlltool").map(PathBuf::from).unwrap();
            }

            build_implib_file(&ws, &name, &rustc_target, &root_output, &dlltool)?;
        }

        if capi_config.header.enabled {
            let header_name = &capi_config.header.name;
            if capi_config.header.generation {
                build_include_file(&ws, header_name, &version, &root_output, &root_path)?;
            } else {
                copy_prebuilt_include_file(&ws, header_name, &root_output, &root_path)?;
            }
        }

        finger_print.store()?;
    }

    Ok((
        build_targets,
        install_paths,
        capi_config,
        static_libs,
        compile_opts,
    ))
}

pub fn ctest(
    ws: &Workspace,
    config: &Config,
    args: &ArgMatches<'_>,
    build_targets: BuildTargets,
    static_libs: String,
    mut compile_opts: CompileOptions,
) -> CliResult {
    compile_opts.build_config.requested_profile =
        args.get_profile_name(&config, "test", ProfileChecking::Checked)?;
    compile_opts.build_config.mode = CompileMode::Test;

    compile_opts.filter = ops::CompileFilter::new(
        LibRule::Default,   // compile the library, so the unit tests can be run filtered
        FilterRule::none(), // we do not have binaries
        FilterRule::All,    // compile the tests, so the integration tests can be run filtered
        FilterRule::none(), // specify --examples to unit test binaries filtered
        FilterRule::none(), // specify --benches to unit test benchmarks filtered
    );

    compile_opts.target_rustc_args = None;

    let ops = ops::TestOptions {
        no_run: args.is_present("no-run"),
        no_fail_fast: args.is_present("no-fail-fast"),
        compile_opts,
    };

    let test_args = args.value_of("TESTNAME").into_iter();
    let test_args = test_args.chain(args.values_of("args").unwrap_or_default());
    let test_args = test_args.collect::<Vec<_>>();

    use std::ffi::OsString;
    let static_lib_path = build_targets.static_lib.unwrap();
    let builddir = static_lib_path.parent().unwrap();

    let mut cflags = OsString::from("-I");
    cflags.push(builddir);
    cflags.push(" ");
    // We push the full path here to work around macos ld not supporting the -l:{filename} syntax
    cflags.push(static_lib_path);

    // We push the static_libs as CFLAGS as well to avoid mangling the options on msvc
    cflags.push(" ");
    cflags.push(static_libs);

    std::env::set_var("INLINE_C_RS_CFLAGS", cflags);

    let err = ops::run_tests(&ws, &ops, &test_args)?;
    match err {
        None => Ok(()),
        Some(err) => {
            let context = anyhow::format_err!("{}", err.hint(&ws, &ops.compile_opts));
            let e = match err.exit.as_ref().and_then(|e| e.code()) {
                // Don't show "process didn't exit successfully" for simple errors.
                Some(i) if errors::is_simple_exit_code(i) => CliError::new(context, i),
                Some(i) => CliError::new(Error::from(err).context(context), i),
                None => CliError::new(Error::from(err).context(context), 101),
            };
            Err(e)
        }
    }
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
