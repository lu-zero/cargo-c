use cargo::util::command_prelude::ArgMatchesExt;
use cargo::CliResult;
use cargo::Config;

use cargo_c::build::*;
use cargo_c::cli::run_cargo_fallback;
use cargo_c::cli::subcommand_build;
use cargo_c::config::*;

fn main() -> CliResult {
    let mut config = Config::default()?;

    let subcommand = subcommand_build("cbuild", "Build the crate C-API");
    let mut app = clap::command!()
        .dont_collapse_args_in_usage(true)
        .allow_external_subcommands(true)
        .subcommand(subcommand);

    let args = app.clone().get_matches();

    let subcommand_args = match args.subcommand() {
        Some(("cbuild", args)) => args,
        Some((cmd, args)) => {
            return run_cargo_fallback(cmd, args);
        }
        _ => {
            // No subcommand provided.
            app.print_help()?;
            return Ok(());
        }
    };

    if subcommand_args.flag("version") {
        println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    config_configure(&mut config, subcommand_args)?;

    let mut ws = subcommand_args.workspace(&config)?;

    let _ = cbuild(&mut ws, &config, subcommand_args, "dev")?;

    Ok(())
}
