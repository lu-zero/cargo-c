use std::io::Write;
use std::path::PathBuf;

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


/// Configuration required by the command
struct Config {
    /// Build artifacts in release mode, with optimizations
    release: bool,
    /// Build for the target triple or the host system.
    target: Option<String>,
    /// Directory for all generated artifacts with the profile appended.
    targetdir: PathBuf,

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

        let target_directory = opt.targetdir.as_ref().unwrap_or(&meta.target_directory);
        let profile = if opt.release { "release" } else { "debug" };
        let targetdir = target_directory.join(profile);

        let prefix = opt.prefix.unwrap_or("/usr/local".into());
        let libdir = opt.libdir.unwrap_or(prefix.join("lib"));
        let includedir = opt.includedir.unwrap_or(prefix.join("include"));

        Config {
            release: opt.release,
            target: opt.target,
            destdir: opt.destdir,

            targetdir,
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
        let static_libs = get_static_libs_for_target(None, target_dir)?;

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

        Ok(())
    }
}

fn main() -> Result<(), std::io::Error> {
    let opts = Opt::from_args();

    println!("{:?}", opts);

    let cwd = std::env::current_dir()?;
    let wd = opts.projectdir.unwrap_or(cwd);

    let mut cmd = MetadataCommand::new();

    cmd.current_dir(wd);

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
