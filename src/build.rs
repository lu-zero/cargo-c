use std::collections::HashMap;
use std::env;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use cargo::core::compiler::unit_graph::UnitDep;
use cargo::core::compiler::unit_graph::UnitGraph;
use cargo::core::compiler::Unit;
use cargo::core::{compiler::Executor, profiles::Profiles};
use cargo::core::{TargetKind, Workspace};
use cargo::ops::{self, CompileFilter, CompileOptions, FilterRule, LibRule};
use cargo::util::command_prelude::{ArgMatches, ArgMatchesExt, CompileMode, ProfileChecking};
use cargo::{CliError, CliResult, Config};

use anyhow::Error;
use cargo_util::paths::copy;
use semver::Version;

use crate::build_targets::BuildTargets;
use crate::install::InstallPaths;
use crate::pkg_config_gen::PkgConfig;
use crate::target;

/// Build the C header
fn build_include_file(
    ws: &Workspace,
    name: &str,
    version: &Version,
    root_output: &Path,
    root_path: &Path,
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
    root_output: &Path,
    root_path: &Path,
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

fn build_pc_file(name: &str, root_output: &Path, pc: &PkgConfig) -> anyhow::Result<()> {
    let pc_path = root_output.join(&format!("{}.pc", name));

    let mut out = std::fs::File::create(pc_path)?;

    let buf = pc.render();

    out.write_all(buf.as_ref())?;

    Ok(())
}

fn build_pc_files(
    ws: &Workspace,
    filename: &str,
    root_output: &Path,
    pc: &PkgConfig,
) -> anyhow::Result<()> {
    ws.config().shell().status("Building", "pkg-config files")?;
    build_pc_file(filename, root_output, pc)?;
    let pc_uninstalled = pc.uninstalled(&root_output);
    build_pc_file(
        &format!("{}-uninstalled", filename),
        root_output,
        &pc_uninstalled,
    )
}

fn patch_target(
    ws: &mut Workspace,
    libkinds: &[&str],
    capi_config: &CApiConfig,
) -> anyhow::Result<()> {
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
            target.set_name(&capi_config.library.name);
        }
    }

    Ok(())
}

fn patch_capi_feature(compile_opts: &mut CompileOptions, ws: &Workspace) -> anyhow::Result<()> {
    let pkg = ws.current()?;
    let manifest = pkg.manifest();

    if manifest.summary().features().get("capi").is_some() {
        std::rc::Rc::get_mut(&mut compile_opts.cli_features.features)
            .unwrap()
            .insert(FeatureValue::new("capi".into()));
    }

    Ok(())
}

/// Build def file for windows-msvc
fn build_def_file(
    ws: &Workspace,
    name: &str,
    target: &target::Target,
    targetdir: &Path,
) -> anyhow::Result<()> {
    let os = &target.os;
    let env = &target.env;

    if os == "windows" && env == "msvc" {
        ws.config()
            .shell()
            .status("Building", ".def file using dumpbin")?;

        let txt_path = targetdir.join(format!("{}.txt", name));

        let target_str = format!("{}-pc-windows-msvc", &target.arch);
        let mut dumpbin = match cc::windows_registry::find(&target_str, "dumpbin.exe") {
            Some(command) => command,
            None => std::process::Command::new("dumpbin"),
        };

        dumpbin
            .arg("/EXPORTS")
            .arg(targetdir.join(format!("{}.dll", name.replace("-", "_"))));
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
    targetdir: &Path,
    dlltool: &Path,
) -> anyhow::Result<()> {
    let os = &target.os;
    let env = &target.env;

    if os == "windows" {
        let arch = &target.arch;
        if env == "gnu" {
            ws.config()
                .shell()
                .status("Building", "implib using dlltool")?;

            let binutils_arch = match arch.as_str() {
                "x86_64" => "i386:x86-64",
                "x86" => "i386",
                _ => unimplemented!("Windows support for {} is not implemented yet.", arch),
            };

            let mut dlltool_command =
                std::process::Command::new(dlltool.to_str().unwrap_or("dlltool"));
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
            ws.config().shell().status("Building", "implib using lib")?;
            let target_str = format!("{}-pc-windows-msvc", &target.arch);
            let mut lib = match cc::windows_registry::find(&target_str, "lib.exe") {
                Some(command) => command,
                None => std::process::Command::new("lib"),
            };
            let lib_arch = match arch.as_str() {
                "x86_64" => "X64",
                "x86" => "IX86",
                _ => unimplemented!("Windows support for {} is not implemented yet.", arch),
            };
            lib.arg(format!(
                "/DEF:{}",
                targetdir.join(format!("{}.def", name)).display()
            ));
            lib.arg(format!("/MACHINE:{}", lib_arch));
            lib.arg(format!("/NAME:{}.dll", name));
            lib.arg(format!(
                "/OUT:{}",
                targetdir.join(format!("{}.dll.lib", name)).display()
            ));

            let out = lib.output()?;
            if out.status.success() {
                Ok(())
            } else {
                Err(anyhow::anyhow!("Command failed {:?}", lib))
            }
        }
    } else {
        Ok(())
    }
}

struct FingerPrint<'a> {
    name: &'a str,
    root_output: &'a Path,
    build_targets: BuildTargets,
    install_paths: &'a InstallPaths,
    static_libs: &'a str,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Cache {
    hash: String,
    static_libs: String,
}

impl<'a> FingerPrint<'a> {
    fn new(
        name: &'a str,
        root_output: &'a Path,
        build_targets: BuildTargets,
        install_paths: &'a InstallPaths,
    ) -> Self {
        Self {
            name,
            root_output,
            build_targets,
            install_paths,
            static_libs: "",
        }
    }

    fn hash(&self) -> anyhow::Result<Option<String>> {
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

        let hash = hasher.finish();
        // the hash is stored in a toml file which does not support u64 so store
        // it as a string to prevent overflows.
        Ok(Some(hash.to_string()))
    }

    fn path(&self) -> PathBuf {
        // Use the crate name in the cache file as the same target dir
        // may be used to build various libs
        self.root_output
            .join(format!("cargo-c-{}.cache", self.name))
    }

    fn load_previous(&self) -> anyhow::Result<Cache> {
        let mut f = std::fs::File::open(&self.path())?;
        let mut cache_str = String::new();
        f.read_to_string(&mut cache_str)?;
        let cache = toml::de::from_str(&cache_str)?;

        Ok(cache)
    }

    fn is_valid(&self) -> bool {
        match (self.load_previous(), self.hash()) {
            (Ok(prev), Ok(Some(current))) => prev.hash == current,
            _ => false,
        }
    }

    fn store(&self) -> anyhow::Result<()> {
        let mut f = std::fs::File::create(&self.path())?;

        if let Some(hash) = self.hash()? {
            let cache = Cache {
                hash,
                static_libs: self.static_libs.to_owned(),
            };
            let buf = toml::ser::to_vec(&cache)?;
            f.write_all(&buf)?;
        }

        Ok(())
    }
}

pub struct CApiConfig {
    pub header: HeaderCApiConfig,
    pub pkg_config: PkgConfigCApiConfig,
    pub library: LibraryCApiConfig,
    pub install: InstallCApiConfig,
}

pub struct HeaderCApiConfig {
    pub name: String,
    pub subdirectory: String,
    pub generation: bool,
    pub enabled: bool,
}

pub struct PkgConfigCApiConfig {
    pub name: String,
    pub filename: String,
    pub description: String,
    pub version: String,
    pub requires: Option<String>,
    pub requires_private: Option<String>,
    pub strip_include_path_components: usize,
}

pub struct LibraryCApiConfig {
    pub name: String,
    pub version: Version,
    pub install_subdir: Option<String>,
    pub versioning: bool,
    pub rustflags: Vec<String>,
}

pub struct InstallCApiConfig {
    pub include: Vec<InstallTarget>,
}

pub enum InstallTarget {
    Asset(InstallTargetPaths),
    Generated(InstallTargetPaths),
}

#[derive(Clone)]
pub struct InstallTargetPaths {
    /// pattern to feed to glob::glob()
    ///
    /// if the InstallTarget is Asset its root is the the root_path
    /// if the InstallTarget is Generated its root is the root_output
    pub from: String,
    /// The path to be joined to the canonical directory to install the files discovered by the
    /// glob, e.g. `{includedir}/{to}` for includes.
    pub to: String,
}

impl InstallTargetPaths {
    pub fn from_value(value: &toml::value::Value, default_to: &str) -> anyhow::Result<Self> {
        let from = value
            .get("from")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("a from field is required"))?;
        let to = value
            .get("to")
            .and_then(|v| v.as_str())
            .unwrap_or(default_to);

        Ok(InstallTargetPaths {
            from: from.to_string(),
            to: to.to_string(),
        })
    }

    pub fn install_paths(
        &self,
        root: &Path,
    ) -> anyhow::Result<impl Iterator<Item = (PathBuf, PathBuf)>> {
        let pattern = root.join(&self.from);
        let base_pattern = if self.from.contains("/**") {
            pattern
                .iter()
                .take_while(|&c| c != std::ffi::OsStr::new("**"))
                .collect()
        } else {
            pattern.parent().unwrap().to_path_buf()
        };
        let pattern = pattern.to_str().unwrap();
        let to = PathBuf::from(&self.to);
        let g = glob::glob(pattern)?.filter_map(move |p| {
            if let Ok(p) = p {
                if p.is_file() {
                    let from = p;
                    let to = to.join(from.strip_prefix(&base_pattern).unwrap());
                    Some((from, to))
                } else {
                    None
                }
            } else {
                None
            }
        });

        Ok(g)
    }
}

fn load_manifest_capi_config(
    name: &str,
    root_path: &Path,
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

    let subdirectory = header
        .as_ref()
        .and_then(|h| h.get("subdirectory"))
        .map(|v| {
            if let Ok(b) = v.clone().try_into::<bool>() {
                Ok(if b {
                    String::from(name)
                } else {
                    String::from("")
                })
            } else {
                v.clone().try_into::<String>()
            }
        })
        .unwrap_or_else(|| Ok(String::from(name)))?;

    let header = if let Some(ref capi) = capi {
        HeaderCApiConfig {
            name: header
                .as_ref()
                .and_then(|h| h.get("name"))
                .or_else(|| capi.get("header_name"))
                .map(|v| v.clone().try_into())
                .unwrap_or_else(|| Ok(String::from(name)))?,
            subdirectory,
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
            subdirectory: String::from(name),
            generation: true,
            enabled: true,
        }
    };

    let pc = capi.and_then(|v| v.get("pkg_config"));
    let pkg = ws.current().unwrap();
    let mut pc_name = String::from(name);
    let mut pc_filename = String::from(name);
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
    let mut strip_include_path_components = 0;

    if let Some(ref pc) = pc {
        if let Some(override_name) = pc.get("name").and_then(|v| v.as_str()) {
            pc_name = String::from(override_name);
        }
        if let Some(override_filename) = pc.get("filename").and_then(|v| v.as_str()) {
            pc_filename = String::from(override_filename);
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
        strip_include_path_components = pc
            .get("strip_include_path_components")
            .map(|v| v.clone().try_into())
            .unwrap_or_else(|| Ok(0))?
    }

    let pkg_config = PkgConfigCApiConfig {
        name: pc_name,
        filename: pc_filename,
        description,
        version,
        requires,
        requires_private,
        strip_include_path_components,
    };

    let library = capi.and_then(|v| v.get("library"));
    let mut lib_name = String::from(name);
    let mut version = pkg.version().clone();
    let mut install_subdir = None;
    let mut versioning = true;
    let mut rustflags = Vec::new();

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
        if let Some(args) = library.get("rustflags").and_then(|v| v.as_str()) {
            let args = args
                .split(' ')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string);
            rustflags.extend(args);
        }
    }

    let library = LibraryCApiConfig {
        name: lib_name,
        version,
        install_subdir,
        versioning,
        rustflags,
    };

    let default_assets_include = InstallTargetPaths {
        from: "assets/capi/include/**/*".to_string(),
        to: header.subdirectory.clone(),
    };

    let default_generated_include = InstallTargetPaths {
        from: "capi/include/**/*".to_string(),
        to: header.subdirectory.clone(),
    };

    let mut include_targets = vec![
        InstallTarget::Asset(default_assets_include),
        InstallTarget::Generated(default_generated_include),
    ];

    let install = capi.and_then(|v| v.get("install"));
    if let Some(ref install) = install {
        if let Some(includes) = install.get("include") {
            if let Some(assets) = includes.get("asset").and_then(|v| v.as_array()) {
                for asset in assets {
                    let target_paths = InstallTargetPaths::from_value(asset, &header.subdirectory)?;
                    include_targets.push(InstallTarget::Asset(target_paths));
                }
            }

            if let Some(generated) = includes.get("generated").and_then(|v| v.as_array()) {
                for gen in generated {
                    let target_paths = InstallTargetPaths::from_value(gen, &header.subdirectory)?;
                    include_targets.push(InstallTarget::Generated(target_paths));
                }
            }
        }
    }

    let install = InstallCApiConfig {
        include: include_targets,
    };

    Ok(CApiConfig {
        header,
        pkg_config,
        library,
        install,
    })
}

fn compile_options(
    ws: &Workspace,
    config: &Config,
    args: &ArgMatches<'_>,
    default_profile: &str,
    compile_mode: CompileMode,
) -> anyhow::Result<CompileOptions> {
    use cargo::core::compiler::CompileKind;
    let mut compile_opts =
        args.compile_options(config, compile_mode, Some(ws), ProfileChecking::Checked)?;

    compile_opts.build_config.requested_profile =
        args.get_profile_name(config, default_profile, ProfileChecking::Checked)?;

    patch_capi_feature(&mut compile_opts, ws)?;

    compile_opts.filter = CompileFilter::new(
        LibRule::True,
        FilterRule::none(),
        FilterRule::none(),
        FilterRule::none(),
        FilterRule::none(),
    );

    compile_opts.build_config.unit_graph = false;

    let rustc = config.load_global_rustc(Some(ws))?;

    // Always set the target, requested_kinds is a vec of a single element.
    if compile_opts.build_config.requested_kinds[0].is_host() {
        compile_opts.build_config.requested_kinds =
            CompileKind::from_requested_targets(config, &[rustc.host.to_string()])?
    }

    Ok(compile_opts)
}

#[derive(Default)]
struct Exec {
    ran: AtomicBool,
    link_line: Mutex<String>,
}

use cargo::core::*;
use cargo::CargoResult;
use cargo_util::{is_simple_exit_code, ProcessBuilder};

impl Executor for Exec {
    fn exec(
        &self,
        cmd: &ProcessBuilder,
        _id: PackageId,
        _target: &Target,
        _mode: CompileMode,
        on_stdout_line: &mut dyn FnMut(&str) -> CargoResult<()>,
        on_stderr_line: &mut dyn FnMut(&str) -> CargoResult<()>,
    ) -> CargoResult<()> {
        self.ran.store(true, Ordering::Relaxed);
        cmd.exec_with_streaming(
            on_stdout_line,
            &mut |s| {
                #[derive(serde::Deserialize, Debug)]
                struct Message {
                    message: String,
                    level: String,
                }

                if let Ok(msg) = serde_json::from_str::<Message>(s) {
                    // suppress the native-static-libs messages
                    if msg.level == "note" {
                        if msg.message.starts_with("Link against the following native artifacts when linking against this static library") {
                            Ok(())
                        } else if let Some(link_line) = msg.message.strip_prefix("native-static-libs:") {
                            *self.link_line.lock().unwrap() = link_line.to_string();
                            Ok(())
                        } else {
                            on_stderr_line(s)
                        }
                    } else {
                        on_stderr_line(s)
                    }
                } else {
                    on_stderr_line(s)
                }
            },
            false,
        )
        .map(drop)
    }
}

use cargo::core::compiler::{unit_graph, UnitInterner};
use cargo::ops::create_bcx;
use cargo::util::profile;

fn set_deps_args(
    dep: &UnitDep,
    graph: &UnitGraph,
    extra_compiler_args: &mut HashMap<Unit, Vec<String>>,
    global_args: &[String],
) {
    if !dep.unit_for.is_for_host() {
        for dep in graph[&dep.unit].iter() {
            set_deps_args(dep, graph, extra_compiler_args, global_args);
        }
        extra_compiler_args
            .entry(dep.unit.clone())
            .or_insert_with(|| global_args.to_owned());
    }
}

fn compile_with_exec<'a>(
    ws: &Workspace<'a>,
    options: &CompileOptions,
    exec: &Arc<dyn Executor>,
    leaf_args: &[String],
    global_args: &[String],
) -> CargoResult<String> {
    ws.emit_warnings()?;
    let interner = UnitInterner::new();
    let mut bcx = create_bcx(ws, options, &interner)?;
    let unit_graph = &bcx.unit_graph;
    let extra_compiler_args = &mut bcx.extra_compiler_args;

    for unit in bcx.roots.iter() {
        extra_compiler_args.insert(unit.clone(), leaf_args.to_owned());
        for dep in unit_graph[unit].iter() {
            set_deps_args(dep, unit_graph, extra_compiler_args, global_args);
        }
    }

    if options.build_config.unit_graph {
        unit_graph::emit_serialized_unit_graph(&bcx.roots, &bcx.unit_graph, ws.config())?;
        return Ok(String::new());
    }
    let _p = profile::start("compiling");
    let cx = cargo::core::compiler::Context::new(&bcx)?;

    let r = cx.compile(exec)?;

    // NOTE: right now we have only a single cdylib since we have a single package to build.
    let out_dir = r
        .cdylibs
        .iter()
        .filter_map(|l| {
            l.script_meta.map(|m| {
                r.extra_env.get(&m).unwrap().iter().filter_map(|e| {
                    if e.0 == "OUT_DIR" {
                        Some(e.1.clone())
                    } else {
                        None
                    }
                })
            })
        })
        .flatten()
        .next()
        .unwrap();

    Ok(out_dir)
}

pub fn cbuild(
    ws: &mut Workspace,
    config: &Config,
    args: &ArgMatches<'_>,
    default_profile: &str,
) -> anyhow::Result<(
    BuildTargets,
    InstallPaths,
    CApiConfig,
    String,
    CompileOptions,
)> {
    let rustc = config.load_global_rustc(Some(ws))?;
    let targets = args.targets();
    let target = match targets.len() {
        0 => rustc.host.to_string(),
        1 => targets[0].to_string(),
        _ => {
            anyhow::bail!("Multiple targets not supported yet");
        }
    };

    let rustc_target = target::Target::new(&target)?;
    let libkinds = args.values_of("library-type").map_or_else(
        || match (rustc_target.os.as_str(), rustc_target.env.as_str()) {
            ("none", _) | (_, "musl") => vec!["staticlib"],
            _ => vec!["staticlib", "cdylib"],
        },
        |v| v.collect::<Vec<_>>(),
    );
    let only_staticlib = !libkinds.contains(&"cdylib");

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

    patch_target(ws, &libkinds, &capi_config)?;

    let name = &capi_config.library.name;

    let install_paths = InstallPaths::new(name, args, &capi_config);

    let mut compile_opts = compile_options(ws, config, args, default_profile, CompileMode::Build)?;

    let profiles = Profiles::new(&ws, compile_opts.build_config.requested_profile)?;

    // TODO: there must be a simpler way to get the right path.
    let root_output = ws
        .target_dir()
        .as_path_unlocked()
        .to_path_buf()
        .join(PathBuf::from(target))
        .join(&profiles.get_dir_name());

    let mut leaf_args: Vec<String> = rustc_target
        .shared_object_link_args(&capi_config, &install_paths.libdir, &root_output)
        .into_iter()
        .flat_map(|l| vec!["-C".to_string(), format!("link-arg={}", l)])
        .collect();

    leaf_args.extend(capi_config.library.rustflags.clone());

    leaf_args.push("--cfg".into());
    leaf_args.push("cargo_c".into());

    leaf_args.push("--print".into());
    leaf_args.push("native-static-libs".into());

    if args.is_present("crt-static") {
        leaf_args.push("-C".into());
        leaf_args.push("target-feature=+crt-static".into());
    }

    let mut build_targets =
        BuildTargets::new(&name, &rustc_target, &root_output, &libkinds, &capi_config)?;

    let mut finger_print =
        FingerPrint::new(&name, &root_output, build_targets.clone(), &install_paths);

    let pristine = finger_print.load_previous().is_err();

    if pristine {
        // If the cache is somehow missing force a full rebuild;
        compile_opts.build_config.force_rebuild = true;
    }

    let exec = Arc::new(Exec::default());
    let out_dir = compile_with_exec(
        ws,
        &compile_opts,
        &(exec.clone() as Arc<dyn Executor>),
        &leaf_args,
        &capi_config.library.rustflags,
    )?;

    let out_dir = Path::new(&out_dir);

    build_targets
        .extra
        .setup(&capi_config, &root_path, &out_dir)?;

    if pristine {
        // restore the default to make sure the tests do not trigger a second rebuild.
        compile_opts.build_config.force_rebuild = false;
    }

    let new_build = exec.ran.load(Ordering::Relaxed);
    let mut static_libs = exec.link_line.lock().unwrap().clone();

    // it is a new build, build the additional files and update update the cache
    // if the hash value does not match.
    if new_build && !finger_print.is_valid() {
        finger_print.static_libs = &static_libs;

        let mut pc = PkgConfig::from_workspace(name, &install_paths, args, &capi_config);
        if only_staticlib {
            pc.add_lib(&static_libs);
        }
        pc.add_lib_private(&static_libs);

        build_pc_files(&ws, &capi_config.pkg_config.filename, &root_output, &pc)?;

        if !only_staticlib {
            let lib_name = name;
            build_def_file(&ws, &lib_name, &rustc_target, &root_output)?;

            let mut dlltool = std::env::var_os("DLLTOOL")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("dlltool"));

            // dlltool argument overwrites environment var
            if args.value_of("dlltool").is_some() {
                dlltool = args.value_of("dlltool").map(PathBuf::from).unwrap();
            }

            build_implib_file(&ws, &lib_name, &rustc_target, &root_output, &dlltool)?;
        }

        if capi_config.header.enabled {
            let header_name = &capi_config.header.name;
            if capi_config.header.generation {
                build_include_file(&ws, header_name, &version, &root_output, &root_path)?;
            } else {
                copy_prebuilt_include_file(&ws, header_name, &root_output, &root_path)?;
            }
        }

        if name.contains('-') {
            let from_build_targets = BuildTargets::new(
                &name.replace("-", "_"),
                &rustc_target,
                &root_output,
                &libkinds,
                &capi_config,
            )?;

            if let (Some(from_static_lib), Some(to_static_lib)) = (
                from_build_targets.static_lib.as_ref(),
                build_targets.static_lib.as_ref(),
            ) {
                copy(from_static_lib, to_static_lib)?;
            }
            if let (Some(from_shared_lib), Some(to_shared_lib)) = (
                from_build_targets.shared_lib.as_ref(),
                build_targets.shared_lib.as_ref(),
            ) {
                copy(from_shared_lib, to_shared_lib)?;
            }
        }

        finger_print.store()?;
    } else {
        // It is not a new build, recover the static_libs value from the cache
        static_libs = finger_print.load_previous()?.static_libs;
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
    _capi_config: &CApiConfig,
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
            let e = match err.code {
                // Don't show "process didn't exit successfully" for simple errors.
                Some(i) if is_simple_exit_code(i) => CliError::new(context, i),
                Some(i) => CliError::new(Error::from(err).context(context), i),
                None => CliError::new(Error::from(err).context(context), 101),
            };
            Err(e)
        }
    }
}

// Take the original cargo instance and save it as a separate env var if not already set.
fn setup_env() {
    if env::var("CARGO_C_CARGO").is_err() {
        let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());

        std::env::set_var("CARGO_C_CARGO", cargo);
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

    // Make sure that the env-vars are correctly set at this point.
    setup_env();
    Ok(())
}
