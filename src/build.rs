use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use cargo::core::compiler::{unit_graph::UnitDep, unit_graph::UnitGraph, Executor, Unit};
use cargo::core::profiles::Profiles;
use cargo::core::{FeatureValue, Package, PackageId, Target, TargetKind, Workspace};
use cargo::ops::{self, CompileFilter, CompileOptions, FilterRule, LibRule};
use cargo::util::command_prelude::{ArgMatches, ArgMatchesExt, CompileMode, ProfileChecking};
use cargo::util::interning::InternedString;
use cargo::{CliResult, GlobalContext};

use anyhow::Context as _;
use cargo_util::paths::{copy, create_dir_all, open, read, read_bytes, write};
use implib::def::ModuleDef;
use implib::{Flavor, ImportLibrary, MachineType};
use itertools::Itertools;
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
    ws.gctx()
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
        name.to_uppercase().replace('-', "_"),
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

/// Copy the pre-built C header from the asset directory to the root_dir
fn copy_prebuilt_include_file(
    ws: &Workspace,
    build_targets: &BuildTargets,
    root_output: &Path,
) -> anyhow::Result<()> {
    let mut shell = ws.gctx().shell();
    shell.status("Populating", "uninstalled header directory")?;
    for (from, to) in build_targets.extra.include.iter() {
        let to = root_output.join("include").join(to);
        create_dir_all(to.parent().unwrap())?;
        copy(from, to)?;
    }

    Ok(())
}

fn build_pc_file(name: &str, root_output: &Path, pc: &PkgConfig) -> anyhow::Result<()> {
    let pc_path = root_output.join(format!("{name}.pc"));
    let buf = pc.render();

    write(pc_path, buf)
}

fn build_pc_files(
    ws: &Workspace,
    filename: &str,
    root_output: &Path,
    pc: &PkgConfig,
) -> anyhow::Result<()> {
    ws.gctx().shell().status("Building", "pkg-config files")?;
    build_pc_file(filename, root_output, pc)?;
    let pc_uninstalled = pc.uninstalled(root_output);
    build_pc_file(
        &format!("{filename}-uninstalled"),
        root_output,
        &pc_uninstalled,
    )
}

fn patch_target(
    pkg: &mut Package,
    library_types: LibraryTypes,
    capi_config: &CApiConfig,
) -> anyhow::Result<()> {
    use cargo::core::compiler::CrateType;

    let manifest = pkg.manifest_mut();
    let targets = manifest.targets_mut();

    let mut kinds = Vec::with_capacity(2);

    if library_types.staticlib {
        kinds.push(CrateType::Staticlib);
    }

    if library_types.cdylib {
        kinds.push(CrateType::Cdylib);
    }

    for target in targets.iter_mut().filter(|t| t.is_lib()) {
        target.set_kind(TargetKind::Lib(kinds.to_vec()));
        target.set_name(&capi_config.library.name);
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
    if target.os == "windows" && target.env == "msvc" {
        ws.gctx().shell().status("Building", ".def file")?;

        // Parse the .dll as an object file
        let dll_path = targetdir.join(format!("{}.dll", name.replace('-', "_")));
        let dll_content = std::fs::read(&dll_path)?;
        let dll_file = object::File::parse(&*dll_content)?;

        // Create the .def output file
        let def_file = cargo_util::paths::create(targetdir.join(format!("{name}.def")))?;

        write_def_file(dll_file, def_file)?;
    }

    Ok(())
}

fn write_def_file<W: std::io::Write>(dll_file: object::File, mut def_file: W) -> anyhow::Result<W> {
    use object::read::Object;

    writeln!(def_file, "EXPORTS")?;

    for export in dll_file.exports()? {
        def_file.write_all(export.name())?;
        def_file.write_all(b"\n")?;
    }

    Ok(def_file)
}

/// Build import library for windows
fn build_implib_file(
    ws: &Workspace,
    build_targets: &BuildTargets,
    name: &str,
    target: &target::Target,
    targetdir: &Path,
) -> anyhow::Result<()> {
    if target.os == "windows" {
        ws.gctx().shell().status("Building", "implib")?;

        let def_path = targetdir.join(format!("{name}.def"));
        let def_contents = cargo_util::paths::read(&def_path)?;

        let flavor = match target.env.as_str() {
            "msvc" => Flavor::Msvc,
            _ => Flavor::Gnu,
        };

        let machine_type = match target.arch.as_str() {
            "x86_64" => MachineType::AMD64,
            "x86" => MachineType::I386,
            "aarch64" => MachineType::ARM64,
            _ => {
                return Err(anyhow::anyhow!(
                    "Windows support for {} is not implemented yet.",
                    target.arch
                ))
            }
        };

        let lib_name = build_targets
            .shared_output_file_name()
            .unwrap()
            .into_string()
            .unwrap();
        let implib_path = build_targets.impl_lib.as_ref().unwrap();

        let implib_file = cargo_util::paths::create(implib_path)?;
        write_implib(implib_file, lib_name, machine_type, flavor, &def_contents)?;
    }

    Ok(())
}

fn write_implib<W: std::io::Write + std::io::Seek>(
    mut w: W,
    lib_name: String,
    machine_type: MachineType,
    flavor: Flavor,
    def_contents: &str,
) -> anyhow::Result<W> {
    let mut module_def = ModuleDef::parse(def_contents, machine_type)?;
    module_def.import_name = lib_name;

    let import_library = ImportLibrary::from_def(module_def, machine_type, flavor);

    import_library.write_to(&mut w)?;

    Ok(w)
}

#[derive(Debug)]
struct FingerPrint {
    id: PackageId,
    root_output: PathBuf,
    build_targets: BuildTargets,
    install_paths: InstallPaths,
    static_libs: Vec<String>,
    hasher: DefaultHasher,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Cache {
    hash: String,
    static_libs: Vec<String>,
}

impl FingerPrint {
    fn new(
        id: &PackageId,
        root_output: &Path,
        build_targets: &BuildTargets,
        install_paths: &InstallPaths,
        capi_config: &CApiConfig,
    ) -> Self {
        let mut hasher = DefaultHasher::new();

        capi_config.hash(&mut hasher);

        Self {
            id: id.to_owned(),
            root_output: root_output.to_owned(),
            build_targets: build_targets.clone(),
            install_paths: install_paths.clone(),
            static_libs: vec![],
            hasher,
        }
    }

    fn hash(&self) -> anyhow::Result<Option<String>> {
        let mut hasher = self.hasher.clone();
        self.install_paths.hash(&mut hasher);

        let mut paths: Vec<&PathBuf> = Vec::new();
        if let Some(include) = &self.build_targets.include {
            paths.push(include);
        }
        paths.extend(&self.build_targets.static_lib);
        paths.extend(&self.build_targets.shared_lib);

        for path in paths.iter() {
            if let Ok(buf) = read_bytes(path) {
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
            .join(format!("cargo-c-{}.cache", self.id.name()))
    }

    fn load_previous(&self) -> anyhow::Result<Cache> {
        let mut f = open(self.path())?;
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
        if let Some(hash) = self.hash()? {
            let cache = Cache {
                hash,
                static_libs: self.static_libs.to_owned(),
            };
            let buf = toml::ser::to_string(&cache)?;
            write(self.path(), buf)?;
        }

        Ok(())
    }
}

#[derive(Debug, Hash)]
pub struct CApiConfig {
    pub header: HeaderCApiConfig,
    pub pkg_config: PkgConfigCApiConfig,
    pub library: LibraryCApiConfig,
    pub install: InstallCApiConfig,
}

#[derive(Debug, Hash)]
pub struct HeaderCApiConfig {
    pub name: String,
    pub subdirectory: String,
    pub generation: bool,
    pub enabled: bool,
}

#[derive(Debug, Hash)]
pub struct PkgConfigCApiConfig {
    pub name: String,
    pub filename: String,
    pub description: String,
    pub version: String,
    pub requires: Option<String>,
    pub requires_private: Option<String>,
    pub strip_include_path_components: usize,
}

#[derive(Debug, Hash)]
pub enum VersionSuffix {
    Major,
    MajorMinor,
    MajorMinorPatch,
}

#[derive(Debug, Hash)]
pub struct LibraryCApiConfig {
    pub name: String,
    pub version: Version,
    pub install_subdir: Option<String>,
    pub versioning: bool,
    pub version_suffix_components: Option<VersionSuffix>,
    pub import_library: bool,
    pub rustflags: Vec<String>,
}

impl LibraryCApiConfig {
    pub fn sover(&self) -> String {
        let major = self.version.major;
        let minor = self.version.minor;
        let patch = self.version.patch;

        match self.version_suffix_components {
            None => match (major, minor, patch) {
                (0, 0, patch) => format!("0.0.{patch}"),
                (0, minor, _) => format!("0.{minor}"),
                (major, _, _) => format!("{major}"),
            },
            Some(VersionSuffix::Major) => format!("{major}"),
            Some(VersionSuffix::MajorMinor) => format!("{major}.{minor}"),
            Some(VersionSuffix::MajorMinorPatch) => format!("{major}.{minor}.{patch}"),
        }
    }
}

#[derive(Debug, Default, Hash)]
pub struct InstallCApiConfig {
    pub include: Vec<InstallTarget>,
    pub data: Vec<InstallTarget>,
}

#[derive(Debug, Hash)]
pub enum InstallTarget {
    Asset(InstallTargetPaths),
    Generated(InstallTargetPaths),
}

#[derive(Clone, Debug, Hash)]
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
    pkg: &Package,
    rustc_target: &target::Target,
) -> anyhow::Result<CApiConfig> {
    let name = &pkg
        .manifest()
        .targets()
        .iter()
        .find(|t| t.is_lib())
        .unwrap()
        .crate_name();
    let root_path = pkg.root().to_path_buf();
    let manifest_str = read(&root_path.join("Cargo.toml"))?;
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

    let header = if let Some(capi) = capi {
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

    if let Some(pc) = pc {
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
    let mut version_suffix_components = None;
    let mut import_library = true;
    let mut rustflags = Vec::new();

    if let Some(library) = library {
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

        if let Some(value) = library.get("version_suffix_components") {
            let value = value.as_integer().with_context(|| {
                format!("Value for `version_suffix_components` is not an integer: {value:?}")
            })?;
            version_suffix_components = Some(match value {
                1 => VersionSuffix::Major,
                2 => VersionSuffix::MajorMinor,
                3 => VersionSuffix::MajorMinorPatch,
                _ => anyhow::bail!("Out of range value for version suffix components: {value}"),
            });
        }

        import_library = library
            .get("import_library")
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

    if rustc_target.os == "android" {
        versioning = false;
    }

    let library = LibraryCApiConfig {
        name: lib_name,
        version,
        install_subdir,
        versioning,
        version_suffix_components,
        import_library,
        rustflags,
    };

    let default_assets_include = InstallTargetPaths {
        from: "assets/capi/include/**/*".to_string(),
        to: header.subdirectory.clone(),
    };

    let header_name = if header.name.ends_with(".h") {
        format!("assets/{}", header.name)
    } else {
        format!("assets/{}.h", header.name)
    };

    let default_legacy_asset_include = InstallTargetPaths {
        from: header_name,
        to: header.subdirectory.clone(),
    };

    let default_generated_include = InstallTargetPaths {
        from: "capi/include/**/*".to_string(),
        to: header.subdirectory.clone(),
    };

    let mut include_targets = vec![
        InstallTarget::Asset(default_assets_include),
        InstallTarget::Asset(default_legacy_asset_include),
        InstallTarget::Generated(default_generated_include),
    ];
    let mut data_targets = Vec::new();

    let mut data_subdirectory = name.clone();

    fn custom_install_target_paths(
        root: &toml::Value,
        subdirectory: &str,
        targets: &mut Vec<InstallTarget>,
    ) -> anyhow::Result<()> {
        if let Some(assets) = root.get("asset").and_then(|v| v.as_array()) {
            for asset in assets {
                let target_paths = InstallTargetPaths::from_value(asset, subdirectory)?;
                targets.push(InstallTarget::Asset(target_paths));
            }
        }

        if let Some(generated) = root.get("generated").and_then(|v| v.as_array()) {
            for gen in generated {
                let target_paths = InstallTargetPaths::from_value(gen, subdirectory)?;
                targets.push(InstallTarget::Generated(target_paths));
            }
        }

        Ok(())
    }

    let install = capi.and_then(|v| v.get("install"));
    if let Some(install) = install {
        if let Some(includes) = install.get("include") {
            custom_install_target_paths(includes, &header.subdirectory, &mut include_targets)?;
        }
        if let Some(data) = install.get("data") {
            if let Some(subdir) = data.get("subdirectory").and_then(|v| v.as_str()) {
                data_subdirectory = String::from(subdir);
            }
            custom_install_target_paths(data, &data_subdirectory, &mut data_targets)?;
        }
    }

    let default_assets_data = InstallTargetPaths {
        from: "assets/capi/share/**/*".to_string(),
        to: data_subdirectory.clone(),
    };

    let default_generated_data = InstallTargetPaths {
        from: "capi/share/**/*".to_string(),
        to: data_subdirectory,
    };

    data_targets.extend([
        InstallTarget::Asset(default_assets_data),
        InstallTarget::Generated(default_generated_data),
    ]);

    let install = InstallCApiConfig {
        include: include_targets,
        data: data_targets,
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
    gctx: &GlobalContext,
    args: &ArgMatches,
    profile: InternedString,
    compile_mode: CompileMode,
) -> anyhow::Result<CompileOptions> {
    use cargo::core::compiler::CompileKind;
    let mut compile_opts =
        args.compile_options(gctx, compile_mode, Some(ws), ProfileChecking::Custom)?;

    compile_opts.build_config.requested_profile = profile;

    std::rc::Rc::get_mut(&mut compile_opts.cli_features.features)
        .unwrap()
        .insert(FeatureValue::new("capi".into()));

    compile_opts.filter = CompileFilter::new(
        LibRule::True,
        FilterRule::none(),
        FilterRule::none(),
        FilterRule::none(),
        FilterRule::none(),
    );

    compile_opts.build_config.unit_graph = false;

    let rustc = gctx.load_global_rustc(Some(ws))?;

    // Always set the target, requested_kinds is a vec of a single element.
    if compile_opts.build_config.requested_kinds[0].is_host() {
        compile_opts.build_config.requested_kinds =
            CompileKind::from_requested_targets(gctx, &[rustc.host.to_string()])?
    }

    Ok(compile_opts)
}

#[derive(Default)]
struct Exec {
    ran: AtomicBool,
    link_line: Mutex<HashMap<PackageId, String>>,
}

use cargo::CargoResult;
use cargo_util::ProcessBuilder;

impl Executor for Exec {
    fn exec(
        &self,
        cmd: &ProcessBuilder,
        id: PackageId,
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
                            self.link_line.lock().unwrap().insert(id, link_line.to_string());
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

fn compile_with_exec(
    ws: &Workspace<'_>,
    options: &CompileOptions,
    exec: &Arc<dyn Executor>,
    rustc_target: &target::Target,
    root_output: &Path,
    args: &ArgMatches,
) -> CargoResult<HashMap<PackageId, PathBuf>> {
    ws.emit_warnings()?;
    let interner = UnitInterner::new();
    let mut bcx = create_bcx(ws, options, &interner)?;
    let unit_graph = &bcx.unit_graph;
    let extra_compiler_args = &mut bcx.extra_compiler_args;

    for unit in bcx.roots.iter() {
        let pkg = &unit.pkg;
        let capi_config = load_manifest_capi_config(pkg, rustc_target)?;
        let name = &capi_config.library.name;
        let install_paths = InstallPaths::new(name, rustc_target, args, &capi_config);
        let pkg_rustflags = &capi_config.library.rustflags;

        let mut leaf_args: Vec<String> = rustc_target
            .shared_object_link_args(&capi_config, &install_paths.libdir, root_output)
            .into_iter()
            .flat_map(|l| ["-C".to_string(), format!("link-arg={l}")])
            .collect();

        leaf_args.extend(pkg_rustflags.clone());

        leaf_args.push("--cfg".into());
        leaf_args.push("cargo_c".into());

        leaf_args.push("--print".into());
        leaf_args.push("native-static-libs".into());

        if args.flag("crt-static") {
            leaf_args.push("-C".into());
            leaf_args.push("target-feature=+crt-static".into());
        }

        extra_compiler_args.insert(unit.clone(), leaf_args.to_owned());

        for dep in unit_graph[unit].iter() {
            set_deps_args(dep, unit_graph, extra_compiler_args, pkg_rustflags);
        }
    }

    if options.build_config.unit_graph {
        unit_graph::emit_serialized_unit_graph(&bcx.roots, &bcx.unit_graph, ws.gctx())?;
        return Ok(HashMap::new());
    }
    let cx = cargo::core::compiler::BuildRunner::new(&bcx)?;

    let r = cx.compile(exec)?;

    let out_dirs = r
        .cdylibs
        .iter()
        .filter_map(|l| {
            let id = l.unit.pkg.package_id();
            if let Some(ref m) = l.script_meta {
                if let Some(env) = r.extra_env.get(m) {
                    env.iter().find_map(|e| {
                        if e.0 == "OUT_DIR" {
                            Some((id, PathBuf::from(&e.1)))
                        } else {
                            None
                        }
                    })
                } else {
                    None
                }
            } else {
                None
            }
        })
        .collect();

    Ok(out_dirs)
}

#[derive(Debug)]
pub struct CPackage {
    pub version: Version,
    pub root_path: PathBuf,
    pub capi_config: CApiConfig,
    pub build_targets: BuildTargets,
    pub install_paths: InstallPaths,
    finger_print: FingerPrint,
}

impl CPackage {
    fn from_package(
        pkg: &mut Package,
        args: &ArgMatches,
        library_types: LibraryTypes,
        rustc_target: &target::Target,
        root_output: &Path,
    ) -> anyhow::Result<CPackage> {
        let id = pkg.package_id();
        let version = pkg.version().clone();
        let root_path = pkg.root().to_path_buf();
        let capi_config = load_manifest_capi_config(pkg, rustc_target)?;

        patch_target(pkg, library_types, &capi_config)?;

        let name = &capi_config.library.name;

        let install_paths = InstallPaths::new(name, rustc_target, args, &capi_config);
        let build_targets = BuildTargets::new(
            name,
            rustc_target,
            root_output,
            library_types,
            &capi_config,
            args.get_flag("meson"),
        )?;

        let finger_print = FingerPrint::new(
            &id,
            root_output,
            &build_targets,
            &install_paths,
            &capi_config,
        );

        Ok(CPackage {
            version,
            root_path,
            capi_config,
            build_targets,
            install_paths,
            finger_print,
        })
    }
}

fn deprecation_warnings(ws: &Workspace, args: &ArgMatches) -> anyhow::Result<()> {
    if args.contains_id("dlltool") {
        ws.gctx()
        .shell()
        .warn("The `dlltool` support is now builtin. The cli option is deprecated and will be removed in the future")?;
    }

    Ok(())
}

/// What library types to build
#[derive(Debug, Clone, Copy)]
pub struct LibraryTypes {
    pub staticlib: bool,
    pub cdylib: bool,
}

impl LibraryTypes {
    fn from_target(target: &target::Target) -> Self {
        // for os == "none", cdylib does not make sense. By default cdylib is also not built on
        // musl, but that can be overriden by the user. That is useful when musl is being used as
        // main libc, e.g. in Alpine, Gentoo and OpenWRT
        //
        // See also
        //
        // - https://github.com/lu-zero/cargo-c?tab=readme-ov-file#shared-libraries-are-not-built-on-musl-systems
        // - https://github.com/lu-zero/cargo-c/issues/180
        Self {
            staticlib: true,
            cdylib: target.os != "none",
        }
    }

    fn from_args(target: &target::Target, args: &ArgMatches) -> Self {
        match args.get_many::<String>("library-type") {
            Some(library_types) => Self::from_library_types(target, library_types),
            None => Self::from_target(target),
        }
    }

    pub(crate) fn from_library_types<S: AsRef<str>>(
        target: &target::Target,
        library_types: impl Iterator<Item = S>,
    ) -> Self {
        let (mut staticlib, mut cdylib) = (false, false);

        for library_type in library_types {
            staticlib |= library_type.as_ref() == "staticlib";
            cdylib |= library_type.as_ref() == "cdylib";
        }

        // when os is none, a cdylib cannot be produced
        // forcing a cdylib for musl is allowed here (see [`LibraryTypes::from_target`])
        cdylib &= target.os != "none";

        Self { staticlib, cdylib }
    }

    const fn only_staticlib(self) -> bool {
        self.staticlib && !self.cdylib
    }

    const fn only_cdylib(self) -> bool {
        self.cdylib && !self.staticlib
    }
}

fn static_libraries(link_line: &str, rustc_target: &target::Target) -> Vec<String> {
    let libs = link_line
        .trim()
        .split(' ')
        .filter(|s| {
            if rustc_target.env == "msvc" && s.starts_with("/defaultlib") {
                return false;
            }
            !s.is_empty()
        })
        .map(|lib| {
            if rustc_target.env == "msvc" && lib.ends_with(".lib") {
                return format!("-l{}", lib.trim_end_matches(".lib"));
            }
            lib.trim().to_string()
        })
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();

    let mut final_libs: Vec<String> = vec![];

    let mut iter = libs.iter();

    // See pkg_config::Library::parse_libs_cflags
    // Reconstitute improperly split lines
    while let Some(part) = iter.next() {
        match part.as_str() {
            "-framework" => {
                if let Some(lib) = iter.next() {
                    final_libs.push(format!("-framework {}", lib));
                }
            }
            "-isystem" | "-iquote" | "-idirafter" => {
                if let Some(inc) = iter.next() {
                    final_libs.push(format!("{} {}", part, inc));
                }
            }
            "-undefined" | "--undefined" => {
                if let Some(symbol) = iter.next() {
                    final_libs.push(format!("-Wl,{},{}", part, symbol));
                }
            }
            _ => final_libs.push(part.to_string()),
        }
    }

    final_libs.into_iter().unique().collect()
}

pub fn cbuild(
    ws: &mut Workspace,
    config: &GlobalContext,
    args: &ArgMatches,
    default_profile: &str,
) -> anyhow::Result<(Vec<CPackage>, CompileOptions)> {
    deprecation_warnings(ws, args)?;

    let (target, is_target_overridden) = match args.targets()?.as_slice() {
        [] => (config.load_global_rustc(Some(ws))?.host.to_string(), false),
        [target] => (target.to_string(), true),
        [..] => anyhow::bail!("Multiple targets not supported yet"),
    };

    let rustc_target = target::Target::new(Some(&target), is_target_overridden)?;

    let library_types = LibraryTypes::from_args(&rustc_target, args);

    let profile = args.get_profile_name(default_profile, ProfileChecking::Custom)?;

    let profiles = Profiles::new(ws, profile)?;

    let mut compile_opts = compile_options(ws, config, args, profile, CompileMode::Build)?;

    // TODO: there must be a simpler way to get the right path.
    let root_output = ws
        .target_dir()
        .as_path_unlocked()
        .to_path_buf()
        .join(PathBuf::from(target))
        .join(profiles.get_dir_name());

    let mut members = Vec::new();
    let mut pristine = false;

    let requested: Vec<_> = compile_opts
        .spec
        .get_packages(ws)?
        .iter()
        .map(|p| p.package_id())
        .collect();

    let capi_feature = InternedString::new("capi");
    let is_relevant_package = |package: &Package| {
        package.library().is_some()
            && package.summary().features().contains_key(&capi_feature)
            && requested.contains(&package.package_id())
    };

    for m in ws.members_mut().filter(|p| is_relevant_package(p)) {
        let cpkg = CPackage::from_package(m, args, library_types, &rustc_target, &root_output)?;

        pristine |= cpkg.finger_print.load_previous().is_err() || !cpkg.finger_print.is_valid();

        members.push(cpkg);
    }

    // If the cache is somehow missing force a full rebuild;
    compile_opts.build_config.force_rebuild |= pristine;

    let exec = Arc::new(Exec::default());
    let out_dirs = compile_with_exec(
        ws,
        &compile_opts,
        &(exec.clone() as Arc<dyn Executor>),
        &rustc_target,
        &root_output,
        args,
    )?;

    for cpkg in members.iter_mut() {
        let out_dir = out_dirs.get(&cpkg.finger_print.id).map(|p| p.as_path());

        cpkg.build_targets
            .extra
            .setup(&cpkg.capi_config, &cpkg.root_path, out_dir)?;

        if cpkg.capi_config.header.generation {
            let mut header_name = PathBuf::from(&cpkg.capi_config.header.name);
            header_name.set_extension("h");
            let from = root_output.join(&header_name);
            let to = Path::new(&cpkg.capi_config.header.subdirectory).join(&header_name);
            cpkg.build_targets.extra.include.push((from, to));
        }
    }

    if pristine {
        // restore the default to make sure the tests do not trigger a second rebuild.
        compile_opts.build_config.force_rebuild = false;
    }

    let new_build = exec.ran.load(Ordering::Relaxed);

    for cpkg in members.iter_mut() {
        // it is a new build, build the additional files and update update the cache
        if new_build {
            let name = &cpkg.capi_config.library.name;
            let (pkg_config_static_libs, static_libs) = if library_types.only_cdylib() {
                (vec![String::new()], vec![String::new()])
            } else if let Some(libs) = exec.link_line.lock().unwrap().get(&cpkg.finger_print.id) {
                (
                    static_libraries(libs, &rustc_target),
                    vec![libs.to_string()],
                )
            } else {
                (vec![String::new()], vec![String::new()])
            };
            let capi_config = &cpkg.capi_config;
            let build_targets = &cpkg.build_targets;

            let mut pc = PkgConfig::from_workspace(name, &cpkg.install_paths, args, capi_config);
            if library_types.only_staticlib() {
                for lib in &pkg_config_static_libs {
                    pc.add_lib(lib);
                }
            }
            for lib in pkg_config_static_libs {
                pc.add_lib_private(&lib);
            }

            build_pc_files(ws, &capi_config.pkg_config.filename, &root_output, &pc)?;

            if !library_types.only_staticlib() && capi_config.library.import_library {
                let lib_name = name;
                build_def_file(ws, lib_name, &rustc_target, &root_output)?;
                build_implib_file(ws, build_targets, lib_name, &rustc_target, &root_output)?;
            }

            if capi_config.header.enabled {
                let header_name = &capi_config.header.name;
                if capi_config.header.generation {
                    build_include_file(
                        ws,
                        header_name,
                        &cpkg.version,
                        &root_output,
                        &cpkg.root_path,
                    )?;
                }

                copy_prebuilt_include_file(ws, build_targets, &root_output)?;
            }

            if name.contains('-') {
                let from_build_targets = BuildTargets::new(
                    &name.replace('-', "_"),
                    &rustc_target,
                    &root_output,
                    library_types,
                    capi_config,
                    args.get_flag("meson"),
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
                if let (Some(from_debug_info), Some(to_debug_info)) = (
                    from_build_targets.debug_info.as_ref(),
                    build_targets.debug_info.as_ref(),
                ) {
                    copy(from_debug_info, to_debug_info)?;
                }
            }

            // This can be supplied to Rust, so it must be in
            // linker-native syntax
            cpkg.finger_print.static_libs = static_libs;
            cpkg.finger_print.store()?;
        } else {
            // It is not a new build, recover the static_libs value from the cache
            cpkg.finger_print.static_libs = cpkg.finger_print.load_previous()?.static_libs;
        }

        ws.gctx().shell().verbose(|s| {
            let path = &format!("PKG_CONFIG_PATH=\"{}\"", root_output.display());
            s.note(path)
        })?;
    }

    Ok((members, compile_opts))
}

pub fn ctest(
    ws: &Workspace,
    args: &ArgMatches,
    packages: &[CPackage],
    mut compile_opts: CompileOptions,
) -> CliResult {
    compile_opts.build_config.requested_profile =
        args.get_profile_name("test", ProfileChecking::Custom)?;
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
        no_run: args.flag("no-run"),
        no_fail_fast: args.flag("no-fail-fast"),
        compile_opts,
    };

    let test_args = args.get_one::<String>("TESTNAME").into_iter();
    let test_args = test_args.chain(args.get_many::<String>("args").unwrap_or_default());
    let test_args = test_args.map(String::as_str).collect::<Vec<_>>();

    use std::ffi::OsString;

    let mut cflags = OsString::new();

    for pkg in packages {
        let static_lib_path = pkg.build_targets.static_lib.as_ref().unwrap();
        let builddir = static_lib_path.parent().unwrap();

        cflags.push("-I");
        cflags.push(builddir);
        cflags.push(" ");

        // We push the full path here to work around macos ld not supporting the -l:{filename} syntax
        cflags.push(static_lib_path);

        // We push the static_libs as CFLAGS as well to avoid mangling the options on msvc
        cflags.push(" ");
        cflags.push(pkg.finger_print.static_libs.join(" "));
    }

    std::env::set_var("INLINE_C_RS_CFLAGS", cflags);

    ops::run_tests(ws, &ops, &test_args)
}

#[cfg(test)]
mod tests {
    use super::*;
    use semver::Version;

    fn make_test_library_config(version: &str) -> LibraryCApiConfig {
        LibraryCApiConfig {
            name: "example".to_string(),
            version: Version::parse(version).unwrap(),
            install_subdir: None,
            versioning: true,
            version_suffix_components: None,
            import_library: true,
            rustflags: vec![],
        }
    }

    #[test]
    pub fn test_semver_zero_zero_zero() {
        let library = make_test_library_config("0.0.0");
        let sover = library.sover();
        assert_eq!(sover, "0.0.0");
    }

    #[test]
    pub fn test_semver_zero_one_zero() {
        let library = make_test_library_config("0.1.0");
        let sover = library.sover();
        assert_eq!(sover, "0.1");
    }

    #[test]
    pub fn test_semver_one_zero_zero() {
        let library = make_test_library_config("1.0.0");
        let sover = library.sover();
        assert_eq!(sover, "1");
    }

    #[test]
    pub fn text_one_fixed_zero_zero_zero() {
        let mut library = make_test_library_config("0.0.0");
        library.version_suffix_components = Some(VersionSuffix::Major);
        let sover = library.sover();
        assert_eq!(sover, "0");
    }

    #[test]
    pub fn text_two_fixed_one_zero_zero() {
        let mut library = make_test_library_config("1.0.0");
        library.version_suffix_components = Some(VersionSuffix::MajorMinor);
        let sover = library.sover();
        assert_eq!(sover, "1.0");
    }

    #[test]
    pub fn text_three_fixed_one_zero_zero() {
        let mut library = make_test_library_config("1.0.0");
        library.version_suffix_components = Some(VersionSuffix::MajorMinorPatch);
        let sover = library.sover();
        assert_eq!(sover, "1.0.0");
    }

    #[test]
    pub fn test_lib_listing() {
        let libs_osx = "-lSystem -lc -lm";
        let libs_linux = "-lgcc_s -lutil -lrt -lpthread -lm -ldl -lc";
        let libs_hurd = "-lgcc_s -lutil -lrt -lpthread -lm -ldl -lc";
        let libs_msvc = "kernel32.lib advapi32.lib kernel32.lib ntdll.lib userenv.lib ws2_32.lib kernel32.lib ws2_32.lib kernel32.lib msvcrt.lib /defaultlib:msvcrt";
        let libs_mingw = "-lkernel32 -ladvapi32 -lkernel32 -lntdll -luserenv -lws2_32 -lkernel32 -lws2_32 -lkernel32";

        let target_osx = target::Target::new(Some("x86_64-apple-darwin"), false).unwrap();
        let target_linux = target::Target::new(Some("x86_64-unknown-linux-gnu"), false).unwrap();
        let target_hurd = target::Target::new(Some("x86_64-unknown-hurd-gnu"), false).unwrap();
        let target_msvc = target::Target::new(Some("x86_64-pc-windows-msvc"), false).unwrap();
        let target_mingw = target::Target::new(Some("x86_64-pc-windows-gnu"), false).unwrap();

        assert_eq!(
            static_libraries(libs_osx, &target_osx).join(" "),
            "-lSystem -lc -lm"
        );
        assert_eq!(
            static_libraries(libs_linux, &target_linux).join(" "),
            "-lgcc_s -lutil -lrt -lpthread -lm -ldl -lc"
        );
        assert_eq!(
            static_libraries(libs_hurd, &target_hurd).join(" "),
            "-lgcc_s -lutil -lrt -lpthread -lm -ldl -lc"
        );
        assert_eq!(
            static_libraries(libs_msvc, &target_msvc).join(" "),
            "-lkernel32 -ladvapi32 -lntdll -luserenv -lws2_32 -lmsvcrt"
        );
        assert_eq!(
            static_libraries(libs_mingw, &target_mingw).join(" "),
            "-lkernel32 -ladvapi32 -lntdll -luserenv -lws2_32"
        );
    }
}
