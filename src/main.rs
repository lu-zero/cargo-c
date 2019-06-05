use std::io::Write;
use std::path::PathBuf;
use std::process::Command as Cmd;

use cargo_metadata::{MetadataCommand, Metadata, Package};
use structopt::StructOpt;

mod pkg_config_gen;
mod static_libs;

use pkg_config_gen::PkgConfig;
use static_libs::get_static_libs_for_target;

#[derive(Debug, StructOpt)]
struct Common {
    /// Build artifacts in release mode, with optimizations
    #[structopt(long = "release")]
    release: bool,
    /// Build for the target triple
    #[structopt(name = "TRIPLE")]
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
}

#[derive(Debug, StructOpt)]
enum Command {
    /// Build C-compatible libraries, headers and pkg-config files
    #[structopt(name = "build")]
    Build {
        #[structopt(flatten)]
        opts: Common,
    },

    /// Install the C-compatible libraries, headers and pkg-config files
    #[structopt(name = "install")]
    Install {
        #[structopt(flatten)]
        opts: Common,
    },
}

#[derive(Debug, StructOpt)]
struct Opt {
    /// Path to the project, by default the current working directory
    #[structopt(long = "projectdir", parse(from_os_str))]
    projectdir: Option<PathBuf>,

    #[structopt(subcommand)]
    cmd: Command,
}

struct Target {
    arch: String,
    vendor: String,
    os: String,
    env: String,
    verbatim: Option<std::ffi::OsString>,
}

impl Target {
    fn new<T: AsRef<std::ffi::OsStr>>(target: Option<T>) -> Result<Self, std::io::Error> {
        let rustc = std::env::var("RUSTC").unwrap_or("rustc".into());
        let mut cmd = Cmd::new(rustc);

        cmd.arg("--print").arg("cfg");

        if let Some(t) = target.as_ref() {
            cmd.arg("--target").arg(t);
        }

        let out = cmd.output()?;
        if out.status.success() {
            fn match_re(re: regex::Regex, s: &str) -> String {
               re
                .captures(s)
                .map_or("", |cap| cap.get(1).unwrap().as_str())
                .to_owned()
            }

            let arch_re = regex::Regex::new(r#"target_arch="(.+)""#).unwrap();
            let vendor_re = regex::Regex::new(r#"target_vendor="(.+)""#).unwrap();
            let os_re = regex::Regex::new(r#"target_os="(.+)""#).unwrap();
            let env_re = regex::Regex::new(r#"target_env="(.+)""#).unwrap();

            let s = std::str::from_utf8(&out.stdout).unwrap();

            Ok(Target {
                arch: match_re(arch_re, s),
                vendor: match_re(vendor_re, s),
                os: match_re(os_re, s),
                env: match_re(env_re, s),
                verbatim: target.map(|v| v.as_ref().to_os_string()),
            })
        } else {
            Err(std::io::ErrorKind::InvalidInput.into())
        }
    }

    fn to_string(&self) -> String {
        let mut s = String::new();
        s.push_str(&self.arch);
        if self.vendor != "" {
            s.push('-');
            s.push_str(&self.vendor);
        }
        if self.os != "" {
            s.push('-');
            s.push_str(&self.os);
        }
        if self.env != "" {
            s.push('-');
            s.push_str(&self.env);
        }
        s
    }
}

/// Configuration required by the command
struct Config {
    /// Build artifacts in release mode, with optimizations
    release: bool,
    /// Build for the target triple or the host system.
    target: Target,
    /// Directory for all generated artifacts with the profile appended.
    targetdir: PathBuf,
    /// Directory for all generated artifacts without the profile appended.
    target_dir: PathBuf,

    destdir: Option<PathBuf>,
    prefix: PathBuf,
    libdir: PathBuf,
    includedir: PathBuf,
    pkg: Package,
}

impl Config {
    fn new(opt: Common, meta: &Metadata) -> Self {
        let pkg = meta
            .packages
            .iter()
            .find(|p| p.id.repr == meta.workspace_members.first().unwrap().repr)
            .unwrap();

        let target_dir = opt.targetdir.as_ref().unwrap_or(&meta.target_directory);
        let profile = if opt.release { "release" } else { "debug" };
        let targetdir = target_dir.join(profile);

        let prefix = opt.prefix.unwrap_or("/usr/local".into());
        let libdir = opt.libdir.unwrap_or(prefix.join("lib"));
        let includedir = opt.includedir.unwrap_or(prefix.join("include"));

        Config {
            release: opt.release,
            target: Target::new(opt.target.as_ref()).unwrap(),
            destdir: opt.destdir,

            targetdir,
            target_dir: target_dir.clone(),
            prefix,
            libdir,
            includedir,
            pkg: pkg.clone(),
        }
    }

    /// Build the pkg-config file
    fn build_pc_file(&self) -> Result<(), std::io::Error> {
        let pkg = &self.pkg;
        let target_dir = &self.targetdir;
        let mut pc = PkgConfig::new(&pkg.name, &pkg.version.to_string());
        let static_libs = get_static_libs_for_target(self.target.verbatim.as_ref(), target_dir)?;

        pc.add_lib_private(static_libs);

        if let Some(descr) = pkg.description.as_ref() {
            pc.set_description(descr);
        }

        let pc_path = target_dir.join(&format!("{}.pc", pkg.name));
        let mut out = std::fs::File::create(pc_path)?;

        let buf = pc.render();

        out.write_all(buf.as_ref())?;

        Ok(())
    }

    /// Build the C header
    fn build_include_file(&self) -> Result<(), std::io::Error> {
        let h_path = self.targetdir.join(&format!("{}.h", self.pkg.name));
        let crate_path = self.pkg.manifest_path.parent().unwrap();

        // TODO: map the errors
        cbindgen::Builder::new().with_crate(crate_path).generate().unwrap().write_to_file(h_path);

        Ok(())
    }

    fn build(&self) -> Result<(), std::io::Error> {
        std::fs::create_dir_all(&self.targetdir)?;

        self.build_pc_file()?;
        self.build_include_file()?;
        self.build_library()?;
        Ok(())
    }

    /// Return a list of linker arguments useful to produce a platform-correct dynamic library
    fn shared_object_link_args(&self) -> Vec<String> {
        let mut lines = Vec::new();
        let name = &self.pkg.name;

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
                "-Wl,--out-implib,{}",
                target_dir.join(format!("{}.dll.a", name)).display()
            ));
            lines.push(format!(
                "-Wl,--output-def,{}",
                target_dir.join(format!("{}.def", name)).display()
            ));
        }

        lines
    }

    /// Build the Library
    fn build_library(&self) -> Result<(), std::io::Error> {
        use std::io;
        let cargo = std::env::var("CARGO").unwrap();
        let mut cmd = std::process::Command::new(cargo);

        cmd.arg("rustc");
        cmd.arg("--target-dir").arg(&self.target_dir);
        cmd.arg("--manifest-path").arg(&self.pkg.manifest_path);

        cmd.arg("--");

        for line in self.shared_object_link_args() {
            cmd.arg("-C").arg(&format!("link-arg={}", line));
        }
        println!("{:?}", cmd);

        let out = cmd.output()?;

        io::stdout().write_all(&out.stdout).unwrap();
        io::stderr().write_all(&out.stderr).unwrap();

        Ok(())
    }
}

fn main() -> Result<(), std::io::Error> {
    let opts = Opt::from_args();

    println!("{:?}", opts);

    let cwd = std::env::current_dir()?;
    let wd = opts.projectdir.unwrap_or(cwd);

    let mut cmd = MetadataCommand::new();


    println!("{:?}", wd);
    cmd.current_dir(&wd);
    cmd.manifest_path(wd.join("Cargo.toml"));

    let meta = cmd.exec().unwrap();

    match opts.cmd {
        Command::Build { opts } => {
            let cfg = Config::new(opts, &meta);
            cfg.build()?;
        }
        _ => unimplemented!(),
    }

    Ok(())
}
