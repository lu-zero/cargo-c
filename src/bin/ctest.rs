use cargo::util::command_prelude::*;
use cargo::CliResult;
use cargo::Config;

use cargo_c::build::*;
use cargo_c::cli::subcommand_test;

use structopt::clap::*;

fn main() -> CliResult {
    let mut config = Config::default()?;

    let subcommand = subcommand_test("ctest");

    let mut app = app_from_crate!()
        .settings(&[
            AppSettings::UnifiedHelpMessage,
            AppSettings::DeriveDisplayOrder,
            AppSettings::VersionlessSubcommands,
        ])
        .subcommand(subcommand);

    let args = app.clone().get_matches();

    let subcommand_args = match args.subcommand() {
        ("ctest", Some(args)) => args,
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

    let (build_targets, _, capi_config, static_libs, compile_opts) =
        cbuild(&mut ws, &config, &subcommand_args, "dev")?;

    ctest(
        &ws,
        &capi_config,
        &config,
        subcommand_args,
        build_targets,
        static_libs,
        compile_opts,
    )
}
