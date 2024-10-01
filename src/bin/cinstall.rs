use cargo::util::command_prelude::ArgMatchesExt;
use cargo::CliResult;
use cargo::GlobalContext;

use cargo_c::build::cbuild;
use cargo_c::cli::{main_cli, run_cargo_fallback, subcommand_install};
use cargo_c::config::global_context_configure;
use cargo_c::install::cinstall;

fn main() -> CliResult {
    let mut config = GlobalContext::default()?;

    let subcommand = subcommand_install("cinstall", "Install the crate C-API");
    let mut app = main_cli().subcommand(subcommand);

    let args = app.clone().get_matches();

    let subcommand_args = match args.subcommand() {
        Some(("cinstall", args)) => args,
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

    global_context_configure(&mut config, subcommand_args)?;

    let mut ws = subcommand_args.workspace(&config)?;

    let (packages, _) = cbuild(&mut ws, &config, subcommand_args, "release")?;

    cinstall(&ws, &packages)?;

    Ok(())
}
