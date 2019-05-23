use std::process::{Command, Stdio};
use std::{env, str};
use std::ffi::OsString;
use std::io::Result;

use regex::Regex;

// TODO: support --target
pub fn get_static_libs_for_target(target: Option<&str>) -> Result<String> {
    let rustc = env::var_os("RUSTC").unwrap_or_else(|| OsString::from("rustc"));

    let mut cmd = Command::new(rustc);
    let cmd = cmd
        .arg("--color")
        .arg("never")
        .arg("--crate-type")
        .arg("staticlib")
        .arg("--print")
        .arg("native-static-libs")
        .arg("-")
        .arg("-o")
        .arg("-")
        .stdin(Stdio::null());

    let out = cmd.output()?;

    if out.status.success() {
        let re = Regex::new(r"note: native-static-libs: (.+)").unwrap();
        let s = str::from_utf8(&out.stderr).unwrap();

        Ok(re.captures(s).map_or("", |cap| cap.get(1).unwrap().as_str()).to_owned())
    } else {
        Err(std::io::ErrorKind::InvalidData.into())
    }
}

pub fn get_static_libs() -> Result<String> {
    get_static_libs_for_target(None)
}

#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn simple() {
        let s = get_static_libs().unwrap();

        println!("libs: {}", s);
    }
}
