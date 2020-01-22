use std::ffi::OsString;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::{env, str};

use anyhow::Result;
use regex::Regex;

pub fn get_static_libs_for_target<T: AsRef<std::ffi::OsStr>>(
    target: Option<T>,
    target_dir: &PathBuf,
) -> Result<String> {
    let rustc = env::var_os("RUSTC").unwrap_or_else(|| OsString::from("rustc"));

    let mut cmd = Command::new(rustc);
    cmd.arg("--color")
        .arg("never")
        .arg("--crate-type")
        .arg("staticlib")
        .arg("--print")
        .arg("native-static-libs")
        .arg("-")
        .arg("--out-dir")
        .arg(target_dir)
        .stdin(Stdio::null());

    if let Some(t) = target {
        cmd.arg("--target").arg(t);
    }

    let out = cmd.output()?;

    log::info!("native-static-libs check {:?} {:?}", cmd, out);

    if out.status.success() {
        let re = Regex::new(r"note: native-static-libs: (.+)").unwrap();
        let s = str::from_utf8(&out.stderr).unwrap();

        Ok(re
            .captures(s)
            .map_or("", |cap| cap.get(1).unwrap().as_str())
            .to_owned())
    } else {
        Err(anyhow::anyhow!("cannot run {:?}", cmd))
    }
}
