#![allow(dead_code)]

use crate::build::CApiConfig;
use crate::install::InstallPaths;
use log::warn;
use pkg_config;
use std::{
    collections::HashSet,
    path::{Component, Path, PathBuf},
};

fn canonicalize<P: AsRef<Path>>(path: P) -> String {
    let mut stack = Vec::with_capacity(16);

    struct Item<'a> {
        separator: bool,
        component: Component<'a>,
    }

    let mut separator = false;

    for component in path.as_ref().components() {
        match component {
            Component::RootDir => {
                separator = true;
            }
            Component::Prefix(_) => stack.push(Item {
                separator: false,
                component,
            }),
            Component::ParentDir => {
                let _ = stack.pop();
            }
            Component::CurDir => stack.push(Item {
                separator: false,
                component,
            }),
            Component::Normal(_) => {
                stack.push(Item {
                    separator,
                    component,
                });
                separator = true;
            }
        }
    }

    if stack.is_empty() {
        String::from("/")
    } else {
        let mut buf = String::with_capacity(64);

        for item in stack {
            if item.separator {
                buf.push('/');
            }

            buf.push_str(&item.component.as_os_str().to_string_lossy());
        }

        buf
    }
}

#[derive(Debug, Clone)]
struct PkgConfigDedupInformation {
    requires: Vec<pkg_config::Library>,
    requires_private: Vec<pkg_config::Library>,
}

#[derive(Debug, Clone)]
pub struct PkgConfig {
    prefix: PathBuf,
    exec_prefix: PathBuf,
    includedir: PathBuf,
    libdir: PathBuf,

    name: String,
    description: String,
    version: String,

    requires: Vec<String>,
    requires_private: Vec<String>,

    libs: Vec<String>,
    libs_private: Vec<String>,

    cflags: Vec<String>,

    conflicts: Vec<String>,

    dedup: PkgConfigDedupInformation,
}

impl PkgConfig {
    ///
    /// Build a pkgconfig structure with the following defaults:
    ///
    /// prefix=/usr/local
    /// exec_prefix=${prefix}
    /// includedir=${prefix}/include
    /// libdir=${exec_prefix}/lib
    ///
    /// Name: $name
    /// Description: $description
    /// Version: $version
    /// Cflags: -I${includedir}/$name
    /// Libs: -L${libdir} -l$name
    ///
    pub fn new(_name: &str, capi_config: &CApiConfig) -> Self {
        let requires = match &capi_config.pkg_config.requires {
            Some(reqs) => reqs.split(',').map(|s| s.trim().to_string()).collect(),
            _ => Vec::new(),
        };

        let requires_libs = {
            let cfg = {
                let mut c = pkg_config::Config::new();
                // This is not sinkholed by cargo-c
                c.env_metadata(false);
                c.cargo_metadata(false);

                c
            };

            // TODO: log which probe fails
            requires
                .iter()
                .flat_map(|req| {
                    let c = cfg.probe(req);
                    if c.is_err() {
                        warn!("WARNING: library not found: {}", c.as_ref().err().unwrap())
                    }
                    c
                })
                .collect::<Vec<_>>()
        };

        let requires_private = match &capi_config.pkg_config.requires_private {
            Some(reqs) => reqs.split(',').map(|s| s.trim().to_string()).collect(),
            _ => Vec::new(),
        };

        let requires_private_libs = {
            let cfg = {
                let mut c = pkg_config::Config::new();
                c.statik(true);
                // This is not sinkholed by cargo-c
                c.env_metadata(false);
                c.cargo_metadata(false);

                c
            };

            // TODO: log which probe fails
            requires_private
                .iter()
                .flat_map(|req| cfg.probe(req))
                .collect::<Vec<_>>()
        };

        let mut libdir = PathBuf::new();
        libdir.push("${libdir}");
        if let Some(subdir) = &capi_config.library.install_subdir {
            libdir.push(subdir);
        }

        let libs = vec![
            format!("-L{}", canonicalize(libdir.display().to_string())),
            format!("-l{}", capi_config.library.name),
        ];

        let cflags = if capi_config.header.enabled {
            let includedir = Path::new("${includedir}").join(&capi_config.header.subdirectory);
            let includedir = includedir
                .ancestors()
                .nth(capi_config.pkg_config.strip_include_path_components)
                .unwrap_or_else(|| Path::new(""));

            format!("-I{}", canonicalize(includedir))
        } else {
            String::from("")
        };

        PkgConfig {
            name: capi_config.pkg_config.name.clone(),
            description: capi_config.pkg_config.description.clone(),
            version: capi_config.pkg_config.version.clone(),

            prefix: "/usr/local".into(),
            exec_prefix: "${prefix}".into(),
            includedir: "${prefix}/include".into(),
            libdir: "${exec_prefix}/lib".into(),

            libs,
            libs_private: Vec::new(),

            requires,
            requires_private,

            cflags: vec![cflags],

            conflicts: Vec::new(),

            dedup: PkgConfigDedupInformation {
                requires: requires_libs,
                requires_private: requires_private_libs,
            },
        }
    }

    pub(crate) fn from_workspace(
        name: &str,
        install_paths: &InstallPaths,
        args: &clap::ArgMatches,
        capi_config: &CApiConfig,
    ) -> Self {
        let mut pc = PkgConfig::new(name, capi_config);

        pc.prefix.clone_from(&install_paths.prefix);
        // TODO: support exec_prefix
        if args.contains_id("includedir") {
            if let Ok(suffix) = install_paths.includedir.strip_prefix(&pc.prefix) {
                pc.includedir = PathBuf::from("${prefix}").join(suffix);
            } else {
                pc.includedir.clone_from(&install_paths.includedir);
            }
        }
        if args.contains_id("libdir") {
            if let Ok(suffix) = install_paths.libdir.strip_prefix(&pc.prefix) {
                pc.libdir = PathBuf::from("${prefix}").join(suffix);
            } else {
                pc.libdir.clone_from(&install_paths.libdir);
            }
        }
        pc
    }

    pub(crate) fn uninstalled(&self, output: &Path) -> Self {
        let mut uninstalled = self.clone();
        uninstalled.prefix = output.to_path_buf();
        uninstalled.includedir = "${prefix}/include".into();
        uninstalled.libdir = "${prefix}".into();
        // First libs item is the search path
        uninstalled.libs[0] = "-L${prefix}".into();

        uninstalled
    }

    pub fn set_description<S: AsRef<str>>(&mut self, descr: S) -> &mut Self {
        descr.as_ref().clone_into(&mut self.description);
        self
    }

    pub fn set_libs<S: AsRef<str>>(&mut self, lib: S) -> &mut Self {
        let lib = lib.as_ref().to_owned();
        self.libs.clear();
        self.libs.push(lib);
        self
    }

    pub fn add_lib<S: AsRef<str>>(&mut self, lib: S) -> &mut Self {
        let lib = lib.as_ref().to_owned();
        self.libs.push(lib);
        self
    }

    pub fn set_libs_private<S: AsRef<str>>(&mut self, lib: S) -> &mut Self {
        let lib = lib.as_ref().to_owned();
        self.libs_private.clear();
        self.libs_private.push(lib);
        self
    }

    pub fn add_lib_private<S: AsRef<str>>(&mut self, lib: S) -> &mut Self {
        let lib = lib.as_ref().to_owned();
        self.libs_private.push(lib);
        self
    }

    pub fn set_cflags<S: AsRef<str>>(&mut self, flag: S) -> &mut Self {
        let flag = flag.as_ref().to_owned();
        self.cflags.clear();
        self.cflags.push(flag);
        self
    }

    pub fn add_cflag<S: AsRef<str>>(&mut self, flag: S) -> &mut Self {
        let flag = flag.as_ref();
        self.cflags.push(flag.to_owned());
        self
    }

    pub fn render(&self) -> String {
        // writing to a String only fails on OOM, which we disregard
        self.render_help(String::with_capacity(1024)).unwrap()
    }

    fn get_libs_cflags(arg: &[pkg_config::Library]) -> (HashSet<String>, HashSet<String>) {
        let mut libs: HashSet<String> = HashSet::new();
        let mut cflags: HashSet<String> = HashSet::new();

        for lib in arg.iter() {
            for lib in lib.include_paths.iter() {
                libs.insert(format!("-I{}", lib.to_string_lossy()));
            }
            for lib in lib.link_files.iter() {
                libs.insert(lib.to_string_lossy().to_string());
            }
            for lib in lib.libs.iter() {
                libs.insert(format!("-l{}", lib));
            }
            for lib in lib.link_paths.iter() {
                libs.insert(format!("-L{}", lib.to_string_lossy()));
            }
            for lib in lib.frameworks.iter() {
                libs.insert(format!("-framework {}", lib));
            }
            for lib in lib.framework_paths.iter() {
                libs.insert(format!("-F{}", lib.to_string_lossy()));
            }
            for lib in lib.defines.iter() {
                let v = match lib.1 {
                    Some(v) => format!("-D{}={}", lib.0, v),
                    None => format!("D{}", lib.0),
                };
                libs.insert(v);
            }
            for lib in lib.ld_args.iter() {
                cflags.insert(format!("-Wl,{}", lib.join(",")));
            }
        }

        (libs, cflags)
    }

    fn dedup_flags(known_flags: &HashSet<String>, flags: &[String]) -> String {
        flags
            .iter()
            .filter(|&lib| {
                !known_flags.contains(lib) || lib.starts_with("-Wl,") || lib.starts_with("/LINK")
            })
            .map(|lib| lib.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn render_help<W: core::fmt::Write>(&self, mut w: W) -> Result<W, core::fmt::Error> {
        // Dedup
        // What libs are already known here?
        let (dedup_cflags, dedup_libs, dedup_libs_private) = {
            let (known_libs, known_cflags) = PkgConfig::get_libs_cflags(&self.dedup.requires);

            let cflags = PkgConfig::dedup_flags(&known_cflags, &self.cflags);
            let libs = PkgConfig::dedup_flags(&known_libs, &self.libs);

            // FIXME: There's no Cflags.private?
            let (mut known_libs_private, _) =
                PkgConfig::get_libs_cflags(&self.dedup.requires_private);
            // Need to be deduplicated against libs too!
            for i in &self.libs {
                known_libs_private.insert(i.clone());
            }

            let libs_private = PkgConfig::dedup_flags(&known_libs_private, &self.libs_private);

            (cflags, libs, libs_private)
        };

        writeln!(w, "prefix={}", canonicalize(&self.prefix))?;
        writeln!(w, "exec_prefix={}", canonicalize(&self.exec_prefix))?;
        writeln!(w, "libdir={}", canonicalize(&self.libdir))?;
        writeln!(w, "includedir={}", canonicalize(&self.includedir))?;

        writeln!(w)?;

        writeln!(w, "Name: {}", self.name)?;
        writeln!(w, "Description: {}", self.description.replace('\n', " "))?; // avoid endlines
        writeln!(w, "Version: {}", self.version)?;
        writeln!(w, "Libs: {}", dedup_libs)?;
        writeln!(w, "Cflags: {}", dedup_cflags)?;

        if !self.libs_private.is_empty() {
            writeln!(w, "Libs.private: {}", dedup_libs_private)?;
        }

        if !self.requires.is_empty() {
            writeln!(w, "Requires: {}", self.requires.join(", "))?;
        }

        if !self.requires_private.is_empty() {
            let joined = self.requires_private.join(", ");
            writeln!(w, "Requires.private: {}", joined)?;
        }

        Ok(w)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use semver::Version;

    #[test]
    fn simple() {
        let mut pkg = PkgConfig::new(
            "foo",
            &CApiConfig {
                header: crate::build::HeaderCApiConfig {
                    name: "foo".into(),
                    subdirectory: "".into(),
                    generation: true,
                    enabled: true,
                },
                pkg_config: crate::build::PkgConfigCApiConfig {
                    name: "foo".into(),
                    filename: "foo".into(),
                    description: "".into(),
                    version: "0.1".into(),
                    requires: Some("somelib, someotherlib".into()),
                    requires_private: Some("someprivatelib >= 1.0".into()),
                    strip_include_path_components: 0,
                },
                library: crate::build::LibraryCApiConfig {
                    name: "foo".into(),
                    version: Version::parse("0.1.0").unwrap(),
                    install_subdir: None,
                    versioning: true,
                    version_suffix_components: None,
                    import_library: true,
                    rustflags: Vec::default(),
                },
                install: Default::default(),
            },
        );
        pkg.add_lib("-lbar").add_cflag("-DFOO");

        let expected = concat!(
            "prefix=/usr/local\n",
            "exec_prefix=${prefix}\n",
            "libdir=${exec_prefix}/lib\n",
            "includedir=${prefix}/include\n",
            "\n",
            "Name: foo\n",
            "Description: \n",
            "Version: 0.1\n",
            "Libs: -L${libdir} -lfoo -lbar\n",
            "Cflags: -I${includedir} -DFOO\n",
            "Requires: somelib, someotherlib\n",
            "Requires.private: someprivatelib >= 1.0\n",
        );

        assert_eq!(expected, pkg.render());
    }

    mod test_canonicalize {
        use super::canonicalize;

        #[test]
        fn test_absolute_path() {
            let path = "/home/user/docs";
            let result = canonicalize(path);
            assert_eq!(result, "/home/user/docs");
        }

        #[test]
        fn test_relative_path() {
            let path = "home/user/docs";
            let result = canonicalize(path);
            assert_eq!(result, "home/user/docs");
        }

        #[test]
        fn test_current_directory() {
            let path = "/home/user/./docs";
            let result = canonicalize(path);
            assert_eq!(result, "/home/user/docs");
        }

        #[test]
        fn test_parent_directory() {
            let path = "/home/user/../docs";
            let result = canonicalize(path);
            assert_eq!(result, "/home/docs");
        }

        #[test]
        fn test_mixed_dots_and_parent_dirs() {
            let path = "/home/./user/../docs/./files";
            let result = canonicalize(path);
            assert_eq!(result, "/home/docs/files");
        }

        #[test]
        fn test_multiple_consecutive_slashes() {
            let path = "/home//user///docs";
            let result = canonicalize(path);
            assert_eq!(result, "/home/user/docs");
        }

        #[test]
        fn test_empty_path() {
            let path = "";
            let result = canonicalize(path);
            assert_eq!(result, "/");
        }

        #[test]
        fn test_single_dot() {
            let path = ".";
            let result = canonicalize(path);
            assert_eq!(result, ".");
        }

        #[test]
        fn test_single_dot_in_absolute_path() {
            let path = "/.";
            let result = canonicalize(path);
            assert_eq!(result, "/");
        }

        #[test]
        fn test_trailing_slash() {
            let path = "/home/user/docs/";
            let result = canonicalize(path);
            assert_eq!(result, "/home/user/docs");
        }

        #[test]
        fn test_dots_complex_case() {
            let path = "/a/b/./c/../d//e/./../f";
            let result = canonicalize(path);
            assert_eq!(result, "/a/b/d/f");
        }

        #[cfg(windows)]
        mod windows {
            use std::path::Path;

            use super::*;

            #[test]
            fn test_canonicalize_basic_windows_path() {
                let input = Path::new(r"C:\Users\test\..\Documents");
                let expected = r"C:/Users/Documents";
                let result = canonicalize(input);
                assert_eq!(result, expected);
            }

            #[test]
            fn test_canonicalize_with_current_dir() {
                let input = Path::new(r"C:\Users\.\Documents");
                let expected = r"C:/Users/Documents";
                let result = canonicalize(input);
                assert_eq!(result, expected);
            }

            #[test]
            fn test_canonicalize_with_double_parent_dir() {
                let input = Path::new(r"C:\Users\test\..\..\Documents");
                let expected = r"C:/Documents";
                let result = canonicalize(input);
                assert_eq!(result, expected);
            }

            #[test]
            fn test_canonicalize_with_trailing_slash() {
                let input = Path::new(r"C:\Users\test\..\Documents\");
                let expected = r"C:/Users/Documents";
                let result = canonicalize(input);
                assert_eq!(result, expected);
            }

            #[test]
            fn test_canonicalize_relative_path() {
                let input = Path::new(r"Users\test\..\Documents");
                let expected = r"Users/Documents";
                let result = canonicalize(input);
                assert_eq!(result, expected);
            }

            #[test]
            fn test_canonicalize_current_dir_only() {
                let input = Path::new(r".\");
                let expected = r".";
                let result = canonicalize(input);
                assert_eq!(result, expected);
            }
        }
    }
}
