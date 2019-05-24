use std::io::Write;
use std::path::PathBuf;

use cargo_metadata::{ MetadataCommand, Package };
use structopt::StructOpt;

mod pkg_config_gen;
mod static_libs;

use pkg_config_gen::PkgConfig;
use static_libs::get_static_libs_for_target;

#[derive(Debug, StructOpt)]
enum Command {
    /// Build C-compatible libraries, headers and pkg-config files
    #[structopt(name = "build")]
    Build {},

    /// Install the C-compatible libraries, headers and pkg-config files
    #[structopt(name = "install")]
    Install {
        #[structopt(long = "destdir", parse(from_os_str))]
        destdir: Option<PathBuf>,
        #[structopt(long = "prefix", parse(from_os_str))]
        prefix: Option<PathBuf>,
        #[structopt(long = "libdir", parse(from_os_str))]
        libdir: Option<PathBuf>,
        #[structopt(long = "includedir", parse(from_os_str))]
        includedir: Option<PathBuf>,
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
        Command::Build {} => {
            build(package, &meta.target_directory)?;
        }
        _ => unimplemented!(),
    }

    Ok(())
}
