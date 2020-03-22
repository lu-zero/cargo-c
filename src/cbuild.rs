use cargo::util::command_prelude::opt;
use cargo::util::command_prelude::{AppExt, ArgMatchesExt};
use cargo::CliResult;
use cargo::Config;

use cargo_c::build::*;
use cargo_c::cli::base_cli;

use structopt::clap::*;

pub fn cli() -> App<'static, 'static> {
    let subcommand = base_cli()
        .name("cbuild")
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
        );

    app_from_crate!()
        .settings(&[
            AppSettings::UnifiedHelpMessage,
            AppSettings::DeriveDisplayOrder,
            AppSettings::VersionlessSubcommands,
            AppSettings::AllowExternalSubcommands,
        ])
        .subcommand(subcommand)
}

fn main() -> CliResult {
    let mut config = Config::default()?;

    let args = cli().get_matches();

    let subcommand_args = match args.subcommand() {
        ("cbuild", Some(args)) => args,
        _ => {
            // No subcommand provided.
            cli().print_help()?;
            return Ok(());
        }
    };

    config_configure(&mut config, subcommand_args)?;

    let mut ws = subcommand_args.workspace(&config)?;

    let _ = cbuild(&mut ws, &config, &subcommand_args)?;

    Ok(())
}
