use std::io::Write;
use std::path::PathBuf;

use cargo_metadata::{MetadataCommand, Package};
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

fn build(pkg: &Package, target_dir: &PathBuf) -> Result<(), std::io::Error> {
    std::fs::create_dir_all(target_dir)?;

    let mut pc = PkgConfig::new(&pkg.name, &pkg.version.to_string());
    let static_libs = get_static_libs_for_target(None, target_dir)?;

    pc.add_lib_private(static_libs);

    if let Some(descr) = pkg.description.as_ref() {
        pc.set_description(descr);
    }

    let pc_path = target_dir.join(&format!("{}.pc", pkg.name));
    println!("path {:?}", pc_path);
    let mut out = std::fs::File::create(pc_path)?;

    let buf = pc.render();

    out.write_all(buf.as_ref())?;

    println!("{:?}", pkg.manifest_path.parent());

    let h_path = target_dir.join(&format!("{}.h", pkg.name));
    let crate_path = pkg.manifest_path.parent().unwrap();

    // TODO: map the errors
    cbindgen::Builder::new().with_crate(crate_path).generate().unwrap().write_to_file(h_path);

    Ok(())
}

fn main() -> Result<(), std::io::Error> {
    let opts = Opt::from_args();

    println!("{:?}", opts);

    let cwd = std::env::current_dir()?;
    let wd = opts.projectdir.unwrap_or(cwd);

    let mut cmd = MetadataCommand::new();

    cmd.current_dir(wd);

    let meta = cmd.exec().unwrap();

    let package = meta
        .packages
        .iter()
        .find(|p| p.id.repr == meta.workspace_members.first().unwrap().repr)
        .unwrap();

    // println!("{:?} {:?}", project.name(), package);

    match opts.cmd {
        Command::Build { opts } => {
            let target_directory = opts.targetdir.as_ref().unwrap_or(&meta.target_directory);
            let profile = if opts.release { "release" } else { "debug" };
            let target_path = target_directory.join(profile);

            build(package, &target_path)?;
        }
        _ => unimplemented!(),
    }

    Ok(())
}
