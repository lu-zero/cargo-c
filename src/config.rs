use std::env;

use cargo::util::command_prelude::{ArgMatches, ArgMatchesExt};
use cargo::{CliResult, GlobalContext};

// Take the original cargo instance and save it as a separate env var if not already set.
fn setup_env() {
    if env::var("CARGO_C_CARGO").is_err() {
        let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());

        env::set_var("CARGO_C_CARGO", cargo);
    }
}

pub fn global_context_configure(gctx: &mut GlobalContext, args: &ArgMatches) -> CliResult {
    let arg_target_dir = &args.value_of_path("target-dir", gctx);
    let config_args: Vec<_> = args
        .get_many::<String>("config")
        .unwrap_or_default()
        .map(|s| s.to_owned())
        .collect();
    gctx.configure(
        args.verbose(),
        args.flag("quiet"),
        args.get_one::<String>("color").map(String::as_str),
        args.flag("frozen"),
        args.flag("locked"),
        args.flag("offline"),
        arg_target_dir,
        &args
            .get_many::<String>("unstable-features")
            .unwrap_or_default()
            .map(|s| s.to_owned())
            .collect::<Vec<String>>(),
        &config_args,
    )?;

    // Make sure that the env-vars are correctly set at this point.
    setup_env();
    Ok(())
}
