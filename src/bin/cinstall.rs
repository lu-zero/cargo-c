use cargo::util::command_prelude::ArgMatchesExt;
use cargo::CliResult;
use cargo::Config;

use cargo_c::build::{cbuild, config_configure};
use cargo_c::cli::subcommand_install;
use cargo_c::install::cinstall;

use structopt::clap::*;

fn main() -> CliResult {
    let mut config = Config::default()?;

    let subcommand = subcommand_install("cinstall", "Install the crate C-API");
    let mut app = app_from_crate!()
        .settings(&[
            AppSettings::UnifiedHelpMessage,
            AppSettings::DeriveDisplayOrder,
            AppSettings::VersionlessSubcommands,
        ])
        .subcommand(subcommand);

    let args = app.clone().get_matches();

    let subcommand_args = match args.subcommand() {
        ("cinstall", Some(args)) => args,
        _ => {
            // No subcommand provided.
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

    let (build_targets, install_paths, capi_config, _, _) =
        cbuild(&mut ws, &config, &subcommand_args, "release")?;

    cinstall(&ws, &capi_config, build_targets, install_paths)?;

    Ok(())
}
