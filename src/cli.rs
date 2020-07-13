use std::path::PathBuf;

use cargo::util::command_prelude::AppExt;
use cargo::util::command_prelude::{multi_opt, opt};

use structopt::clap::*;
use structopt::StructOpt;

// TODO: convert to a function using cargo opt()
#[derive(Clone, Debug, StructOpt)]
struct Common {
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
    #[structopt(long = "dlltool", parse(from_os_str))]
    /// Use the provided dlltool when building for the windows-gnu targets.
    dlltool: Option<PathBuf>,
}

fn base_cli() -> App<'static, 'static> {
    Common::clap()
        .arg(opt("version", "Print version info and exit").short("V"))
        .arg(
            opt(
                "verbose",
                "Use verbose output (-vv very verbose/build.rs output)",
            )
            .short("v")
            .multiple(true)
            .global(true),
        )
        .arg(opt("quiet", "No output printed to stdout").short("q"))
        .arg(
            opt("color", "Coloring: auto, always, never")
                .value_name("WHEN")
                .global(true),
        )
        .arg(opt("frozen", "Require Cargo.lock and cache are up to date").global(true))
        .arg(opt("locked", "Require Cargo.lock is up to date").global(true))
        .arg(opt("offline", "Run without accessing the network").global(true))
        .arg(
            multi_opt("config", "KEY=VALUE", "Override a configuration value")
                .global(true)
                .hidden(true),
        )
        .arg(
            multi_opt(
                "library-type",
                "LIBRARY-TYPE",
                "Build only a type of library",
            )
            .global(true)
            .case_insensitive(true)
            .possible_values(&["cdylib", "staticlib"]),
        )
}

pub fn subcommand_cli(name: &str) -> App<'static, 'static> {
    base_cli()
        .name(name)
        .arg_jobs()
        .arg_release("Build artifacts in release mode, with optimizations")
        .arg_profile("Build artifacts with the specified profile")
        .arg_features()
        .arg_target_triple("Build for the target triple")
        .arg_target_dir()
        .arg(
            opt(
                "out-dir",
                "Copy final artifacts to this directory (unstable)",
            )
            .value_name("PATH"),
        )
        .arg_manifest_path()
        .arg_message_format()
        .arg_build_plan()
        .after_help(
            "
Compilation can be configured via the use of profiles which are configured in
the manifest. The default profile for this command is `dev`, but passing
the --release flag will use the `release` profile instead.
",
        )
}
