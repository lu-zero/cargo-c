use cargo_c::build::{cbuild, config_configure};
use cargo_c::cli::subcommand_cli;
use cargo_c::install::cinstall;
use cargo_c::target::Target;

use cargo::util::command_prelude::opt;
use cargo::util::command_prelude::ArgMatchesExt;
use cargo::CliResult;
use cargo::Config;

use structopt::clap::*;

fn main() -> CliResult {
    let mut config = Config::default()?;

    let cli_build = subcommand_cli("build", "Build the crate C-API");
    let cli_install = subcommand_cli("install", "Install the crate C-API");

    let mut app = app_from_crate!()
        .settings(&[
            AppSettings::UnifiedHelpMessage,
            AppSettings::DeriveDisplayOrder,
            AppSettings::VersionlessSubcommands,
        ])
        .subcommand(
            SubCommand::with_name("capi")
                .about("Build or install the crate C-API")
                .arg(opt("version", "Print version info and exit").short("V"))
                .subcommand(cli_build)
                .subcommand(cli_install),
        );

    let args = app.clone().get_matches();

    let (cmd, subcommand_args) = match args.subcommand() {
        ("capi", Some(args)) => match args.subcommand() {
            (cmd, Some(args)) if cmd == "build" || cmd == "install" => (cmd, args),
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

    let (build_targets, install_paths) = cbuild(&mut ws, &config, &subcommand_args)?;

    if cmd == "install" {
        cinstall(
            &ws,
            &Target::new(subcommand_args.target())?,
            build_targets,
            install_paths,
        )?;
    }

    Ok(())
}
