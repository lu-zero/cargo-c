use std::path::PathBuf;
use std::io::Write;

use cargo_project::{Artifact, Profile, Project};
use structopt::StructOpt;

mod pkg_config_gen;
mod static_libs;

use pkg_config_gen::PkgConfig;
use static_libs::get_static_libs_for_target;

#[derive(Debug, StructOpt)]
enum Command {
    /// Build C-compatible libraries, headers and pkg-config files
    #[structopt(name = "build")]
    Build {
    },

    /// Install the C-compatible libraries, headers and pkg-config files
    #[structopt(name = "install")]
    Install {
        #[structopt(long="destdir", parse(from_os_str))]
        destdir: Option<PathBuf>,
        #[structopt(long="prefix", parse(from_os_str))]
        prefix: Option<PathBuf>,
        #[structopt(long="libdir", parse(from_os_str))]
        libdir: Option<PathBuf>,
        #[structopt(long="includedir", parse(from_os_str))]
        includedir: Option<PathBuf>,
    },
}

#[derive(Debug, StructOpt)]
struct Opt {
    /// Path to the project, by default the current working directory
    #[structopt(long="projectdir", parse(from_os_str))]
    projectdir: Option<PathBuf>,

   #[structopt(subcommand)]
    cmd: Command,
}

trait Build {
    fn build(&self) -> Result<(), std::io::Error>;
}

impl Build for Project {
    fn build(&self) -> Result<(), std::io::Error> {
        let mut pc = PkgConfig::new(self.name(), "unimplemented", "unimplemented");
        let static_libs = get_static_libs_for_target(self.target())?;
        let pc = pc.add_lib_private(static_libs);

        let pc_path = self.target_dir().to_path_buf();

        let pc_path = pc_path.join(&format!("{}.pc", self.name()));

        let mut out = std::fs::File::create(pc_path)?;

        let buf = pc.render();

        out.write_all(buf.as_ref())?;

        Ok(())
    }
}

fn main() -> Result<(), std::io::Error> {
    let opts = Opt::from_args();

    println!("{:?}", opts);

    let cwd = std::env::current_dir()?;
    let wd = opts.projectdir.unwrap_or(cwd);
    let project = Project::query(wd).expect("Cannot find the cargo project");

    match opts.cmd {
        Command::Build { } => {
            project.build()?;
        },
        _ => unimplemented!(),
    }

    Ok(())
}
