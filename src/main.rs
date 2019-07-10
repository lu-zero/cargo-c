use std::io::Write;
use std::path::PathBuf;

use cargo_metadata::{MetadataCommand, Package};
use log::*;
use structopt::StructOpt;

mod pkg_config_gen;
mod static_libs;

use pkg_config_gen::PkgConfig;
use static_libs::get_static_libs_for_target;

#[derive(Clone, Debug, StructOpt)]
struct Common {
    /// Path to the project, by default the current working directory
    #[structopt(long = "project-dir", parse(from_os_str))]
    projectdir: Option<PathBuf>,
    /// Number of rustc jobs to run in parallel.
    #[structopt(long = "jobs")]
    jobs: Option<u32>,
    /// Build artifacts in release mode, with optimizations
    #[structopt(long = "release")]
    release: bool,
    /// Build for the target triple
    #[structopt(long = "target", name = "TRIPLE")]
    target: Option<String>,
    /// Directory for all generated artifacts
    #[structopt(name = "DIRECTORY", long = "target-dir", parse(from_os_str))]
    targetdir: Option<PathBuf>,

    #[structopt(long = "destdir", parse(from_os_str))]
    destdir: Option<PathBuf>,
    #[structopt(long = "prefix", parse(from_os_str))]
    prefix: Option<PathBuf>,
    #[structopt(long = "libdir", parse(from_os_str))]
    libdir: Option<PathBuf>,
    #[structopt(long = "includedir", parse(from_os_str))]
    includedir: Option<PathBuf>,
    #[structopt(long = "bindir", parse(from_os_str))]
    bindir: Option<PathBuf>,
    #[structopt(long = "pkgconfigdir", parse(from_os_str))]
    pkgconfigdir: Option<PathBuf>,

    /// Space-separated list of features to activate
    #[structopt(long = "features")]
    features: Option<String>,
    /// Activate all available features
    #[structopt(long = "all-features")]
    allfeatures: bool,
    /// Do not activate the `default` feature
    #[structopt(long = "no-default-features")]
    nodefaultfeatures: bool,
}

#[derive(Debug, StructOpt)]
enum Command {
    /// Build C-compatible libraries, headers and pkg-config files
    #[structopt(name = "build", alias = "cbuild")]
    Build {
        #[structopt(flatten)]
        opts: Common,
    },

    /// Install the C-compatible libraries, headers and pkg-config files
    #[structopt(name = "install", alias = "cinstall")]
    Install {
        #[structopt(flatten)]
        opts: Common,
    },
}

#[derive(Debug, StructOpt)]
struct Opt {
    #[structopt(subcommand)]
    cmd: Command,
}

/// Split a target string to its components
///
/// Because of https://github.com/rust-lang/rust/issues/61558
/// It uses internally `rustc` to validate the string.
struct Target {
    arch: String,
    // vendor: String,
    os: String,
    env: String,
    verbatim: Option<std::ffi::OsString>,
}

impl Target {
    fn new<T: AsRef<std::ffi::OsStr>>(target: Option<T>) -> Result<Self, std::io::Error> {
        let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".into());
        let mut cmd = std::process::Command::new(rustc);

        cmd.arg("--print").arg("cfg");

        if let Some(t) = target.as_ref() {
            cmd.arg("--target").arg(t);
        }

        let out = cmd.output()?;
        if out.status.success() {
            fn match_re(re: regex::Regex, s: &str) -> String {
                re.captures(s)
                    .map_or("", |cap| cap.get(1).unwrap().as_str())
                    .to_owned()
            }

            let arch_re = regex::Regex::new(r#"target_arch="(.+)""#).unwrap();
            // let vendor_re = regex::Regex::new(r#"target_vendor="(.+)""#).unwrap();
            let os_re = regex::Regex::new(r#"target_os="(.+)""#).unwrap();
            let env_re = regex::Regex::new(r#"target_env="(.+)""#).unwrap();

            let s = std::str::from_utf8(&out.stdout).unwrap();

            Ok(Target {
                arch: match_re(arch_re, s),
                // vendor: match_re(vendor_re, s),
                os: match_re(os_re, s),
                env: match_re(env_re, s),
                verbatim: target.map(|v| v.as_ref().to_os_string()),
            })
        } else {
            Err(std::io::ErrorKind::InvalidInput.into())
        }
    }
}

/// Files we are expected to produce
///
/// Currently we produce only 1 header, 1 pc file and a variable number of
/// files for the libraries.
#[derive(Debug)]
struct BuildTargets {
    include: PathBuf,
    static_lib: PathBuf,
    shared_lib: PathBuf,
    impl_lib: Option<PathBuf>,
    def: Option<PathBuf>,
    pc: PathBuf,
}

impl BuildTargets {
    fn new(cfg: &Config, hash: &str) -> BuildTargets {
        let name = &cfg.name;

        let pc = cfg.targetdir.join(&format!("{}.pc", name));
        let include = cfg.targetdir.join(&format!("{}.h", name));

        let os = &cfg.target.os;
        let env = &cfg.target.env;

        let targetdir = cfg.targetdir.join("deps");

        let (shared_lib, static_lib, impl_lib, def) = match (os.as_str(), env.as_str()) {
            ("linux", _) => {
                let static_lib = targetdir.join(&format!("lib{}-{}.a", name, hash));
                let shared_lib = targetdir.join(&format!("lib{}-{}.so", name, hash));
                (shared_lib, static_lib, None, None)
            }
            ("macos", _) => {
                let static_lib = targetdir.join(&format!("lib{}-{}.a", name, hash));
                let shared_lib = targetdir.join(&format!("lib{}-{}.dylib", name, hash));
                (shared_lib, static_lib, None, None)
            }
            ("windows", "gnu") => {
                let static_lib = targetdir.join(&format!("{}-{}.lib", name, hash));
                let shared_lib = targetdir.join(&format!("{}-{}.dll", name, hash));
                let impl_lib = cfg.targetdir.join(&format!("{}.dll.a", name));
                let def = cfg.targetdir.join(&format!("{}.def", name));
                (shared_lib, static_lib, Some(impl_lib), Some(def))
            }
            _ => unimplemented!("The target {}-{} is not supported yet", os, env),
        };

        BuildTargets {
            pc,
            include,
            static_lib,
            shared_lib,
            impl_lib,
            def,
        }
    }
}

use serde_derive::*;

/// cargo fingerpring of the target crate
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct BuildInfo {
    hash: String,
}

/// Configuration required by the command
struct Config {
    /// The library name
    name: String,
    /// Build artifacts in release mode, with optimizations
    release: bool,
    /// Build for the target triple or the host system.
    target: Target,
    /// Directory for all generated artifacts with the profile appended.
    targetdir: PathBuf,
    /// Directory for all generated artifacts without the profile appended.
    target_dir: PathBuf,

    destdir: PathBuf,
    prefix: PathBuf,
    libdir: PathBuf,
    includedir: PathBuf,
    bindir: PathBuf,
    pkgconfigdir: PathBuf,
    pkg: Package,

    /// Cargo binary to call
    cargo: PathBuf,

    /// Features to pass to the inner call
    features: Option<String>,
    allfeatures: bool,
    nodefaultfeatures: bool,
    cli: Common,
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

impl Config {
    fn new(opt: Common) -> Self {
        let cli = opt.clone();
        let wd = opt
            .projectdir
            .unwrap_or_else(|| std::env::current_dir().unwrap());

        let mut cmd = MetadataCommand::new();

        cmd.current_dir(&wd);
        cmd.manifest_path(wd.join("Cargo.toml"));

        let meta = cmd.exec().unwrap();

        let pkg = meta
            .packages
            .iter()
            .find(|p| p.id.repr == meta.workspace_members.first().unwrap().repr)
            .unwrap();

        let target_dir = opt.targetdir.as_ref().unwrap_or(&meta.target_directory);
        let profile = if opt.release { "release" } else { "debug" };
        let targetdir = target_dir.join(profile);

        let prefix = opt.prefix.unwrap_or_else(|| "/usr/local".into());
        let libdir = opt.libdir.unwrap_or_else(|| prefix.join("lib"));
        let includedir = opt.includedir.unwrap_or_else(|| prefix.join("include"));
        let bindir = opt.bindir.unwrap_or_else(|| prefix.join("bin"));
        let pkgconfigdir = opt.pkgconfigdir.unwrap_or_else(|| libdir.join("pkgconfig"));

        let name = pkg
            .targets
            .iter()
            .find(|t| t.kind.iter().any(|x| x == "lib"))
            .expect("Cannot find a library target")
            .name
            .clone();

        Config {
            name,
            release: opt.release,
            target: Target::new(opt.target.as_ref()).unwrap(),
            destdir: opt.destdir.unwrap_or_else(|| PathBuf::from("/")),

            targetdir,
            target_dir: target_dir.clone(),
            prefix,
            libdir,
            includedir,
            bindir,
            pkgconfigdir,
            pkg: pkg.clone(),
            cargo: std::env::var("CARGO")
                .unwrap_or_else(|_| "cargo".into())
                .into(),
            features: opt.features,
            allfeatures: opt.allfeatures,
            nodefaultfeatures: opt.nodefaultfeatures,
            cli,
        }
    }

    fn open_build_info(&self) -> Option<BuildInfo> {
        let info_path = self.targetdir.join(".cargo-c.toml");
        let mut f = std::fs::File::open(info_path).ok()?;

        use std::io::Read;
        let mut s = Vec::new();

        f.read_to_end(&mut s).ok()?;

        let t = toml::from_slice::<BuildInfo>(&s).unwrap();

        info!("saved build hash {}", t.hash);

        Some(t)
    }

    fn save_build_info(&self, info: &BuildInfo) {
        let info_path = self.targetdir.join(".cargo-c.toml");
        let mut f = std::fs::File::create(info_path).unwrap();
        let s = toml::to_vec(info).unwrap();

        f.write_all(&s).unwrap();
    }

    /// Build the pkg-config file
    fn build_pc_file(&self, build_targets: &BuildTargets) -> Result<(), std::io::Error> {
        log::info!("Building PkgConfig for {}", build_targets.pc.display());
        let mut pc = PkgConfig::from_config(&self);
        let target_dir = &self.targetdir;
        let static_libs = get_static_libs_for_target(self.target.verbatim.as_ref(), target_dir)?;

        pc.add_lib_private(static_libs);

        let pc_path = &build_targets.pc;
        let mut out = std::fs::File::create(pc_path)?;

        let buf = pc.render();

        out.write_all(buf.as_ref())?;

        Ok(())
    }

    /// Build import library for Windows
    fn build_implib_file(&self) -> Result<(), std::io::Error> {
        log::info!("Building implib using dlltool for target {}",
                   self.target.verbatim.as_ref().map(|os| os.to_string_lossy().into_owned()).unwrap_or("native".into()));
        let os = &self.target.os;
        let env = &self.target.env;

        if os == "windows" && env == "gnu" {
            let name = &self.name;
            let arch = &self.target.arch;
            let target_dir = &self.targetdir;

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
                .arg(target_dir.join(format!("{}.dll.a", name)));
            dlltool
                .arg("-d")
                .arg(target_dir.join(format!("{}.def", name)));

            let out = dlltool.output()?;
            if out.status.success() {
                Ok(())
            } else {
                Err(std::io::ErrorKind::InvalidInput.into())
            }
        } else {
            Ok(())
        }
    }

    /// Build the C header
    fn build_include_file(&self, build_targets: &BuildTargets) -> Result<(), std::io::Error> {
        log::info!("Building include file using cbindgen");
        let include_path = &build_targets.include;
        let crate_path = self.pkg.manifest_path.parent().unwrap();

        // TODO: map the errors
        let config = cbindgen::Config::from_root_or_default(crate_path);
        cbindgen::Builder::new()
            .with_crate(crate_path)
            .with_config(config)
            .generate()
            .unwrap()
            .write_to_file(include_path);

        Ok(())
    }

    /// Return a list of linker arguments useful to produce a platform-correct dynamic library
    fn shared_object_link_args(&self) -> Vec<String> {
        let mut lines = Vec::new();
        let name = &self.name;

        let major = self.pkg.version.major;
        let minor = self.pkg.version.minor;
        let patch = self.pkg.version.patch;

        let os = &self.target.os;
        let env = &self.target.env;

        let libdir = &self.libdir;
        let target_dir = &self.targetdir;

        if os == "linux" {
            lines.push(format!("-Wl,-soname,lib{}.so.{}", name, major));
        } else if os == "macos" {
            let line = format!("-Wl,-install_name,{1}/lib{0}.{2}.{3}.{4}.dylib,-current_version,{2}.{3}.{4},-compatibility_version,{2}",
                    name, libdir.display(), major, minor, patch);
            lines.push(line)
        } else if os == "windows" && env == "gnu" {
            // This is only set up to work on GNU toolchain versions of Rust
            lines.push(format!(
                "-Wl,--output-def,{}",
                target_dir.join(format!("{}.def", name)).display()
            ));
        }

        lines
    }

    /// Build the Library
    fn build_library(&self) -> Result<Option<BuildInfo>, std::io::Error> {
        log::info!("Building the libraries using cargo rustc");
        use std::io;
        let mut cmd = std::process::Command::new(&self.cargo);

        cmd.arg("rustc");
        cmd.arg("-v");
        cmd.arg("--lib");
        cmd.arg("--target-dir").arg(&self.target_dir);
        cmd.arg("--manifest-path").arg(&self.pkg.manifest_path);

        if let Some(jobs) = self.cli.jobs {
            cmd.arg("--jobs").arg(jobs.to_string());
        }

        if let Some(t) = self.target.verbatim.as_ref() {
            cmd.arg("--target").arg(t);
        }

        if self.release {
            cmd.arg("--release");
        }

        if let Some(features) = self.features.as_ref() {
            cmd.arg("--features").arg(features);
        }

        if self.allfeatures {
            cmd.arg("--all-features");
        }

        if self.nodefaultfeatures {
            cmd.arg("--no-default-features");
        }

        cmd.arg("--");

        cmd.arg("--crate-type").arg("staticlib");
        cmd.arg("--crate-type").arg("cdylib");

        cmd.arg("--cfg").arg("cargo_c");

        for line in self.shared_object_link_args() {
            cmd.arg("-C").arg(&format!("link-arg={}", line));
        }
        info!("build_library {:?}", cmd);

        let out = cmd.output()?;

        io::stdout().write_all(&out.stdout).unwrap();
        io::stderr().write_all(&out.stderr).unwrap();
        // TODO: replace this hack with something saner
        let exp = &format!(".* -C extra-filename=-([^ ]*) .*");
        // println!("exp : {}", exp);
        let re = regex::Regex::new(exp).unwrap();
        let s = std::str::from_utf8(&out.stderr).unwrap();

        let fresh_line = format!("Fresh {} ", self.pkg.name);

        let is_fresh = s.lines().rfind(|line| line.contains(&fresh_line)).is_some();

        if !is_fresh {
            let s = s
                .lines()
                .rfind(|line| line.contains("--cfg cargo_c"))
                .unwrap();

            let hash = re
                .captures(s)
                .map(|cap| cap.get(1).unwrap().as_str())
                .unwrap()
                .to_owned();

            info!("parsed hash {}", hash);

            Ok(Some(BuildInfo { hash }))
        } else {
            Ok(None)
        }
    }

    fn build(&self) -> Result<BuildInfo, std::io::Error> {
        log::info!("Building");
        std::fs::create_dir_all(&self.targetdir)?;

        let prev_info = self.open_build_info();

        let mut info = self.build_library()?;

        if info.is_none() && prev_info.is_none() {
            let mut cmd = std::process::Command::new(&self.cargo);
            cmd.arg("clean");

            cmd.status()?;
            info = Some(self.build_library()?.unwrap());
        }

        let info = if prev_info.is_none() || (info.is_some() && info != prev_info) {
            let info = info.unwrap();
            let build_targets = BuildTargets::new(self, &info.hash);

            self.build_pc_file(&build_targets)?;
            self.build_implib_file()?;
            self.build_include_file(&build_targets)?;

            self.save_build_info(&info);
            info
        } else {
            eprintln!("Already built");
            prev_info.unwrap()
        };

        Ok(info)
    }

    fn install(&self, build_targets: BuildTargets) -> Result<(), std::io::Error> {
        log::info!("Installing");
        use std::fs;
        // TODO make sure the build targets exist and are up to date
        // println!("{:?}", self.build_targets);

        let os = &self.target.os;
        let env = &self.target.env;
        let name = &self.name;
        let ver = &self.pkg.version;

        let install_path_lib = append_to_destdir(&self.destdir, &self.libdir);
        let install_path_pc = append_to_destdir(&self.destdir, &self.pkgconfigdir);
        let install_path_include = append_to_destdir(&self.destdir, &self.includedir).join(name);
        let install_path_bin = append_to_destdir(&self.destdir, &self.bindir);

        // fs::create_dir_all(&install_path_lib);
        fs::create_dir_all(&install_path_pc)?;
        fs::create_dir_all(&install_path_include)?;
        fs::create_dir_all(&install_path_bin)?;

        fs::copy(
            &build_targets.pc,
            install_path_pc.join(&format!("{}.pc", name)),
        )?;
        fs::copy(
            &build_targets.include,
            install_path_include.join(&format!("{}.h", name)),
        )?;
        fs::copy(
            &build_targets.static_lib,
            install_path_lib.join(&format!("lib{}.a", name)),
        )?;

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
            ("linux", _) => {
                let lib = &format!("lib{}.so", name);
                let lib_with_major_ver = &format!("{}.{}", lib, ver.major);
                let lib_with_full_ver =
                    &format!("{}.{}.{}", lib_with_major_ver, ver.minor, ver.patch);
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
}

fn main() -> Result<(), std::io::Error> {
    pretty_env_logger::init();
    let opts = Opt::from_args();

    match opts.cmd {
        Command::Build { opts } => {
            let cfg = Config::new(opts);
            cfg.build()?;
        }
        Command::Install { opts } => {
            let cfg = Config::new(opts);

            let info = cfg.build()?;
            let build_targets = BuildTargets::new(&cfg, &info.hash);

            info!("{:?}", build_targets);

            cfg.install(build_targets)?;
        }
    }

    Ok(())
}
