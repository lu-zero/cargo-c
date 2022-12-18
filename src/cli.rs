use std::ffi::{OsStr, OsString};
use std::path::PathBuf;

use cargo::util::command_prelude::CommandExt;
use cargo::util::command_prelude::{flag, multi_opt, opt};
use cargo::util::{CliError, CliResult};

use cargo_util::{ProcessBuilder, ProcessError};

use clap::{Arg, ArgAction, ArgMatches, Command, CommandFactory, Parser};

// TODO: convert to a function using cargo opt()
#[allow(dead_code)]
#[derive(Clone, Debug, Parser)]
struct Common {
    /// Path to directory where target should be copied to
    #[clap(long = "destdir")]
    destdir: Option<PathBuf>,
    /// Directory path used to construct default values of
    /// includedir, libdir, bindir, pkgconfigdir
    #[clap(long = "prefix")]
    prefix: Option<PathBuf>,
    /// Path to directory for installing generated library files
    #[clap(long = "libdir")]
    libdir: Option<PathBuf>,
    /// Path to directory for installing generated headers files
    #[clap(long = "includedir")]
    includedir: Option<PathBuf>,
    /// Path to directory for installing generated executable files
    #[clap(long = "bindir")]
    bindir: Option<PathBuf>,
    /// Path to directory for installing generated pkg-config .pc files
    #[clap(long = "pkgconfigdir")]
    pkgconfigdir: Option<PathBuf>,
    /// Path to directory for installing read-only data (defaults to {prefix}/share)
    #[clap(long = "datarootdir")]
    datarootdir: Option<PathBuf>,
    /// Path to directory for installing read-only application-specific data
    /// (defaults to {datarootdir})
    #[clap(long = "datadir")]
    datadir: Option<PathBuf>,
    #[clap(long = "dlltool")]
    /// Use the provided dlltool when building for the windows-gnu targets.
    dlltool: Option<PathBuf>,
    #[clap(long = "crt-static")]
    /// Build the library embedding the C runtime
    crt_static: bool,
}

fn base_cli() -> Command {
    Common::command()
        .allow_external_subcommands(true)
        .arg(flag("version", "Print version info and exit").short('V'))
        .arg(flag("list", "List installed commands"))
        .arg(opt("explain", "Run `rustc --explain CODE`").value_name("CODE"))
        .arg(
            opt(
                "verbose",
                "Use verbose output (-vv very verbose/build.rs output)",
            )
            .short('v')
            .action(ArgAction::Count)
            .global(true),
        )
        .arg_quiet()
        .arg(
            opt("color", "Coloring: auto, always, never")
                .value_name("WHEN")
                .global(true),
        )
        .arg(flag("frozen", "Require Cargo.lock and cache are up to date").global(true))
        .arg(flag("locked", "Require Cargo.lock is up to date").global(true))
        .arg(flag("offline", "Run without accessing the network").global(true))
        .arg(multi_opt("config", "KEY=VALUE", "Override a configuration value").global(true))
        .arg(
            Arg::new("unstable-features")
                .help("Unstable (nightly-only) flags to Cargo, see 'cargo -Z help' for details")
                .short('Z')
                .value_name("FLAG")
                .action(ArgAction::Append)
                .global(true),
        )
        .arg_jobs()
        .arg_targets_all(
            "Build only this package's library",
            "Build only the specified binary",
            "Build all binaries",
            "Build only the specified example",
            "Build all examples",
            "Build only the specified test target",
            "Build all tests",
            "Build only the specified bench target",
            "Build all benches",
            "Build all targets",
        )
        .arg_profile("Build artifacts with the specified profile")
        .arg_features()
        .arg_target_triple("Build for the target triple")
        .arg_target_dir()
        .arg_manifest_path()
        .arg_message_format()
        .arg_build_plan()
}

pub fn subcommand_build(name: &'static str, about: &'static str) -> Command {
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
            .ignore_case(true)
            .value_parser(["cdylib", "staticlib"]),
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

pub fn subcommand_install(name: &'static str, about: &'static str) -> Command {
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
            .ignore_case(true)
            .value_parser(["cdylib", "staticlib"]),
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

pub fn subcommand_test(name: &'static str) -> Command {
    base_cli()
        .trailing_var_arg(true)
        .name(name)
        .about("Test the crate C-API")
        .arg(
            Arg::new("args")
                .help("Arguments for the test binary")
                .num_args(0..)
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
    let mut args = vec![OsStr::new(subcommand)];

    args.extend(
        subcommand_args
            .get_many::<OsString>("")
            .unwrap_or_default()
            .map(OsStr::new),
    );
    let err = match ProcessBuilder::new(cargo).args(&args).exec_replace() {
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
