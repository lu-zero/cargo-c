use std::path::PathBuf;
use structopt::clap::ArgMatches;

#[derive(Debug)]
pub struct InstallPaths {
    pub destdir: PathBuf,
    pub prefix: PathBuf,
    pub libdir: PathBuf,
    pub includedir: PathBuf,
    pub bindir: PathBuf,
    pub pkgconfigdir: PathBuf,
}

impl InstallPaths {
    pub fn from_matches(args: &ArgMatches<'_>) -> Self {
        let destdir = args
            .value_of("destdir")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/"));
        let prefix = args
            .value_of("prefix")
            .map(PathBuf::from)
            .unwrap_or_else(|| "/usr/local".into());
        let libdir = args
            .value_of("libdir")
            .map(PathBuf::from)
            .unwrap_or_else(|| prefix.join("lib"));
        let includedir = args
            .value_of("includedir")
            .map(PathBuf::from)
            .unwrap_or_else(|| prefix.join("include"));
        let bindir = args
            .value_of("bindir")
            .map(PathBuf::from)
            .unwrap_or_else(|| prefix.join("bin"));
        let pkgconfigdir = args
            .value_of("pkgconfigdir")
            .map(PathBuf::from)
            .unwrap_or_else(|| libdir.join("pkgconfig"));

        InstallPaths {
            destdir,
            prefix,
            libdir,
            includedir,
            bindir,
            pkgconfigdir,
        }
    }
}
