use cargo_metadata::*;
use std::path::*;

fn main() {
    let path = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let meta = MetadataCommand::new()
        .manifest_path("./Cargo.toml")
        .current_dir(&path)
        .exec()
        .unwrap();

    println!("{:?}", meta);

    let out = std::env::var("OUT_DIR").unwrap();
    let out = Path::new(&out);

    let path = out.join("capi/include/");
    let subdir = path.join("subdir");
    let include = out.join("include");

    std::fs::create_dir_all(&path).unwrap();
    std::fs::create_dir_all(&subdir).unwrap();
    std::fs::create_dir_all(&include).unwrap();

    std::fs::write(path.join("generated.h"), "// Generated").unwrap();
    std::fs::write(subdir.join("in_subdir.h"), "// Generated").unwrap();
    std::fs::write(include.join("other_file.h"), "// Generated").unwrap();
}
