use cargo_c::build::{cbuild, ctest};
use cargo_c::cli::*;
use cargo_c::config::*;
use cargo_c::install::cinstall;

use cargo::util::command_prelude::opt;
use cargo::util::command_prelude::ArgMatchesExt;
use cargo::CliResult;
use cargo::Config;

use structopt::clap::*;

fn main() -> CliResult {
    let mut config = Config::default()?;

    let cli_build = subcommand_build("build", "Build the crate C-API");
    let cli_install = subcommand_install("install", "Install the crate C-API");
    let cli_test = subcommand_test("test");

    let mut app = app_from_crate!()
        .settings(&[
            AppSettings::UnifiedHelpMessage,
            AppSettings::DeriveDisplayOrder,
            AppSettings::VersionlessSubcommands,
            AppSettings::AllowExternalSubcommands,
        ])
        .subcommand(
            SubCommand::with_name("capi")
                .about("Build or install the crate C-API")
                .arg(opt("version", "Print version info and exit").short("V"))
                .subcommand(cli_build)
                .subcommand(cli_install)
                .subcommand(cli_test),
        );

    let args = app.clone().get_matches();

    let (cmd, subcommand_args, default_profile) = match args.subcommand() {
        ("capi", Some(args)) => match args.subcommand() {
            ("build", Some(args)) => ("build", args, "dev"),
            ("test", Some(args)) => ("test", args, "dev"),
            ("install", Some(args)) => ("install", args, "release"),
            (cmd, Some(args)) => {
                return run_cargo_fallback(cmd, args);
            }
            _ => {
                // No subcommand provided.
                app.print_help()?;
                return Ok(());
            }
        },
        _ => {
            app.print_help()?;
            return Ok(());
        }
    };

    if subcommand_args.is_present("version") {
        println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    config_configure(&mut config, subcommand_args)?;

    let mut ws = subcommand_args.workspace(&config)?;

    let (packages, compile_opts) = cbuild(&mut ws, &config, subcommand_args, default_profile)?;

    if cmd == "install" {
        cinstall(&ws, &packages)?;
    } else if cmd == "test" {
        ctest(&ws, &config, subcommand_args, &packages, compile_opts)?;
    }

    Ok(())
}
