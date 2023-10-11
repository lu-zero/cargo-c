pub mod build;
pub mod build_targets;
pub mod cli;
pub mod config;
pub mod install;
pub mod pkg_config_gen;
pub mod target;

trait VersionExt {
    /// build the main version string
    fn main_version(&self) -> String;
}

impl VersionExt for semver::Version {
    fn main_version(&self) -> String {
        match (self.major, self.minor, self.patch) {
            (0, 0, patch) => format!("0.0.{patch}"),
            (0, minor, _) => format!("0.{minor}"),
            (major, _, _) => format!("{major}"),
        }
    }
}
