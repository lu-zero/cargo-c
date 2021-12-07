use std::path::PathBuf;

use cargo::util::command_prelude::AppExt;
use cargo::util::command_prelude::{multi_opt, opt};
use cargo::util::{CliError, CliResult};

use cargo_util::{ProcessBuilder, ProcessError};
use structopt::clap::*;
use structopt::StructOpt;

// TODO: convert to a function using cargo opt()
#[allow(dead_code)]
#[derive(Clone, Debug, StructOpt)]
struct Common {
    /// Path to directory where target should be copied to
    #[structopt(long = "destdir", parse(from_os_str))]
    destdir: Option<PathBuf>,
    /// Directory path used to construct default values of
    /// includedir, libdir, bindir, pkgconfigdir
    #[structopt(long = "prefix", parse(from_os_str))]
    prefix: Option<PathBuf>,
    /// Path to directory for installing generated library files
    #[structopt(long = "libdir", parse(from_os_str))]
    libdir: Option<PathBuf>,
    /// Path to directory for installing generated headers files
    #[structopt(long = "includedir", parse(from_os_str))]
    includedir: Option<PathBuf>,
    /// Path to directory for installing generated executable files
    #[structopt(long = "bindir", parse(from_os_str))]
    bindir: Option<PathBuf>,
    /// Path to directory for installing generated pkg-config .pc files
    #[structopt(long = "pkgconfigdir", parse(from_os_str))]
    pkgconfigdir: Option<PathBuf>,
    /// Path to directory for installing read-only data (defaults to {prefix}/share)
    #[structopt(long = "datarootdir", parse(from_os_str))]
    datarootdir: Option<PathBuf>,
    /// Path to directory for installing read-only application-specific data
    /// (defaults to {datarootdir})
    #[structopt(long = "datadir", parse(from_os_str))]
    datadir: Option<PathBuf>,
    #[structopt(long = "dlltool", parse(from_os_str))]
    /// Use the provided dlltool when building for the windows-gnu targets.
    dlltool: Option<PathBuf>,
    #[structopt(long = "crt-static")]
    /// Build the library embedding the C runtime
    crt_static: bool,
}

fn base_cli() -> App<'static, 'static> {
    Common::clap()
        .setting(AppSettings::ColoredHelp)
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
        .arg_jobs()
        .arg_profile("Build artifacts with the specified profile")
        .arg_features()
        .arg_target_triple("Build for the target triple")
        .arg_target_dir()
        .arg_manifest_path()
        .arg_message_format()
        .arg_build_plan()
}

pub fn subcommand_build(name: &str, about: &'static str) -> App<'static, 'static> {
    base_cli()
        .name(name)
        .about(about)
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
        .arg_release("Build artifacts in release mode, with optimizations")
        .arg_package_spec_no_all(
            "Package to build (see `cargo help pkgid`)",
            "Build all packages in the workspace",
            "Exclude packages from the build",
        )
        .after_help(
            "
Compilation can be configured via the use of profiles which are configured in
the manifest. The default profile for this command is `dev`, but passing
the --release flag will use the `release` profile instead.
",
        )
}

pub fn subcommand_install(name: &str, about: &'static str) -> App<'static, 'static> {
    base_cli()
        .name(name)
        .about(about)
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
        .arg(opt("debug", "Build in debug mode instead of release mode"))
        .arg_release(
            "Build artifacts in release mode, with optimizations. This is the default behavior.",
        )
        .arg_package_spec_no_all(
            "Package to install (see `cargo help pkgid`)",
            "Install all packages in the workspace",
            "Exclude packages from being installed",
        )
        .after_help(
            "
Compilation can be configured via the use of profiles which are configured in
the manifest. The default profile for this command is `release`, but passing
the --debug flag will use the `dev` profile instead.
",
        )
}

pub fn subcommand_test(name: &str) -> App<'static, 'static> {
    base_cli()
        .settings(&[AppSettings::TrailingVarArg])
        .name(name)
        .about("Test the crate C-API")
        .arg(
            Arg::with_name("args")
                .help("Arguments for the test binary")
                .multiple(true)
                .last(true),
        )
        .arg_release("Build artifacts in release mode, with optimizations")
        .arg_package_spec_no_all(
            "Package to run tests for",
            "Test all packages in the workspace",
            "Exclude packages from the test",
        )
        .arg(opt("no-run", "Compile, but don't run tests"))
        .arg(opt("no-fail-fast", "Run all tests regardless of failure"))
}

pub fn run_cargo_fallback(subcommand: &str, subcommand_args: &ArgMatches) -> CliResult {
    let cargo = std::env::var("CARGO_C_CARGO").unwrap_or_else(|_| "cargo".to_owned());
    let mut args = vec![subcommand];

    args.extend(subcommand_args.values_of("").unwrap_or_default());
    let err = match ProcessBuilder::new(&cargo).args(&args).exec_replace() {
        Ok(()) => return Ok(()),
        Err(e) => e,
    };

    if let Some(perr) = err.downcast_ref::<ProcessError>() {
        if let Some(code) = perr.code {
            return Err(CliError::code(code));
        }
    }
    Err(CliError::new(err, 101))
}
