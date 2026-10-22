use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn visit(path: &Path, root: &Path, files: &mut Vec<String>) {
    let metadata = fs::symlink_metadata(path).expect("read plugin asset metadata");
    if metadata.file_type().is_symlink() {
        panic!("plugin assets cannot be symlinks: {}", path.display());
    }
    if metadata.is_dir() {
        println!("cargo:rerun-if-changed={}", path.display());
        let mut entries: Vec<_> = fs::read_dir(path)
            .expect("read plugin asset directory")
            .map(|entry| entry.expect("read plugin asset entry").path())
            .collect();
        entries.sort();
        for entry in entries {
            visit(&entry, root, files);
        }
    } else if metadata.is_file() {
        println!("cargo:rerun-if-changed={}", path.display());
        let relative = path
            .strip_prefix(root)
            .expect("plugin asset is under repository root")
            .to_str()
            .expect("plugin asset path is UTF-8")
            .replace('\\', "/");
        files.push(relative);
    } else {
        panic!(
            "plugin assets must be files or directories: {}",
            path.display()
        );
    }
}

fn main() {
    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let root = manifest_dir
        .join("../..")
        .canonicalize()
        .expect("repository root");
    let list = root.join("plugin-files.txt");
    println!("cargo:rerun-if-changed={}", list.display());

    let mut files = Vec::new();
    for line in fs::read_to_string(&list)
        .expect("read plugin-files.txt")
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
    {
        let source = root.join(line);
        if !source.starts_with(&root) {
            panic!("plugin asset path must stay under repository root: {line}");
        }
        visit(&source, &root, &mut files);
    }
    files.sort();
    files.dedup();
    assert!(
        !files.is_empty(),
        "plugin-files.txt must list plugin assets"
    );

    let mut generated = String::from("const PLUGIN_ASSETS: &[(&str, &[u8])] = &[\n");
    for path in files {
        generated.push_str(&format!(
            "    ({path:?}, include_bytes!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/../../\", {path:?}))),\n"
        ));
    }
    generated.push_str("];\n");
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR")).join("plugin_assets.rs");
    fs::write(out, generated).expect("write generated plugin asset list");
}
