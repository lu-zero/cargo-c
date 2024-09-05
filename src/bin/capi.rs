use cargo_c::build::{cbuild, ctest};
use cargo_c::cli::*;
use cargo_c::config::*;
use cargo_c::install::cinstall;

use cargo::util::command_prelude::flag;
use cargo::util::command_prelude::ArgMatchesExt;
use cargo::{CliResult, GlobalContext};

use clap::*;

fn main() -> CliResult {
    let mut config = GlobalContext::default()?;

    let cli_build = subcommand_build("build", "Build the crate C-API");
    let cli_install = subcommand_install("install", "Install the crate C-API");
    let cli_test = subcommand_test("test");

    let mut app = clap::command!()
        .dont_collapse_args_in_usage(true)
        .allow_external_subcommands(true)
        .subcommand(
            Command::new("capi")
                .allow_external_subcommands(true)
                .about("Build or install the crate C-API")
                .arg(flag("version", "Print version info and exit").short('V'))
                .subcommand(cli_build)
                .subcommand(cli_install)
                .subcommand(cli_test),
        );

    let args = app.clone().get_matches();

    let (cmd, subcommand_args, default_profile) = match args.subcommand() {
        Some(("capi", args)) => match args.subcommand() {
            Some(("build", args)) => ("build", args, "dev"),
            Some(("test", args)) => ("test", args, "dev"),
            Some(("install", args)) => ("install", args, "release"),
            Some((cmd, args)) => {
                return run_cargo_fallback(cmd, args);
            }
            _ => {
                // No subcommand provided.
                app.print_help()?;
                return Ok(());
            }
        },
        Some((cmd, args)) => {
            return run_cargo_fallback(cmd, args);
        }
        _ => {
            app.print_help()?;
            return Ok(());
        }
    };

    if subcommand_args.flag("version") {
        println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    global_context_configure(&mut config, subcommand_args)?;

    let mut ws = subcommand_args.workspace(&config)?;

    let (packages, compile_opts) = cbuild(&mut ws, &config, subcommand_args, default_profile)?;

    if cmd == "install" {
        cinstall(&ws, &packages)?;
    } else if cmd == "test" {
        ctest(&ws, subcommand_args, &packages, compile_opts)?;
    }

    Ok(())
}
