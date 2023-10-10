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
        if self.major == 0 {
            if self.minor == 0 {
                format!("{}.{}.{}", self.major, self.minor, self.patch)
            } else {
                format!("{}.{}", self.major, self.minor)
            }
        } else {
            format!("{}", self.major)
        }
    }
}
