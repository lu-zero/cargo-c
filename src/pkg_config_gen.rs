#[derive(Debug, Clone)]
pub struct PkgConfig {
    prefix: String,
    exec_prefix: String,
    includedir: String,
    libdir: String,

    name: String,
    description: String,
    version: String,

    requires: Vec<String>,
    requires_private: Vec<String>,

    libs: Vec<String>,
    libs_private: Vec<String>,

    cflags: Vec<String>,

    conflicts: Vec<String>,
}

impl PkgConfig {
    ///
    /// Build a pkgconfig structure with the following defaults:
    ///
    /// ```
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
    pub fn new<A, B, C>(name: A, version: B, description: C) -> Self
        where A: AsRef<str>,
              B: AsRef<str>,
              C: AsRef<str>,
    {
        let name = name.as_ref();
        let version = version.as_ref();
        let description = description.as_ref();
        PkgConfig {
            name: name.to_owned(),
            version: version.to_owned(),
            description: description.to_owned(),

            prefix: "/usr/local".to_owned(),
            exec_prefix: "${prefix}".to_owned(),
            includedir: "${prefix}/include".to_owned(),
            libdir: "${exec_prefix}/lib".to_owned(),

            libs: vec![format!("-L{} -l{}", "${libdir}", name).to_owned()],
            libs_private: Vec::new(),

            requires: Vec::new(),
            requires_private: Vec::new(),

            cflags: vec![format!("-I{}/{}", "${includedir}", name)],

            conflicts: Vec::new(),
        }
    }

    pub fn set_libs<S: AsRef<str>>(mut self, lib: S) -> Self {
        let lib = lib.as_ref().to_owned();
        self.libs.clear();
        self.libs.push(lib);
        self
    }

    pub fn add_lib<S: AsRef<str>>(mut self, lib: S) -> Self {
        let lib = lib.as_ref().to_owned();
        self.libs.push(lib);
        self
    }

    pub fn set_libs_private<S: AsRef<str>>(mut self, lib: S) -> Self {
        let lib = lib.as_ref().to_owned();
        self.libs.clear();
        self.libs.push(lib);
        self
    }

    pub fn add_lib_private<S: AsRef<str>>(mut self, lib: S) -> Self {
        let lib = lib.as_ref().to_owned();
        self.libs_private.push(lib);
        self
    }

    pub fn set_cflags<S: AsRef<str>>(mut self, flag: S) -> Self {
        let flag = flag.as_ref().to_owned();
        self.libs.clear();
        self.libs.push(flag);
        self
    }

    pub fn add_cflag<S: AsRef<str>>(mut self, flag: S) -> Self {
        let flag = flag.as_ref();
        self.libs.push(flag.to_owned());
        self
    }

    pub fn render(&self) -> String {
        let mut base = format!(
            "prefix={}
exec_prefix={}
libdir={}
includedir={}

Name: {}
Description: {}
Version: {}
Libs: {}
Cflags: {}",
            self.prefix,
            self.exec_prefix,
            self.libdir,
            self.includedir,
            self.name,
            self.description,
            self.version,
            self.libs.join(" "),
            self.cflags.join(" "),
        )
        .to_owned();

        if !self.libs_private.is_empty() {
            base.push_str(&format!("
Libs.private: {}",
self.libs_private.join(" "),
            ));
        }

        if !self.requires.is_empty() {
            base.push_str(&format!(
                "
Requires: {}",
                self.requires.join(", ")
            ));
        }
        /*
        Requires: libavresample >= 4.0.0, libavutil >= 56.8.0
        Requires.private:
        Conflicts:
        Libs.private:

                ).to_owned()
        */

        base.push_str("\n");

        base
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn simple() {
        let pkg = PkgConfig::new("foo", "0.1", "test pc")
            .add_lib("-lbar")
            .add_cflag("-DFOO");

        println!("{:?}\n{}", pkg, pkg.render());
    }
}
