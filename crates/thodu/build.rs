use std::env;
use std::fs;
use std::path::{Component, Path, PathBuf};

/// One listed plugin file. Only the files plugin-files.txt names are
/// embedded, so nothing else under components/ or lib/ (an editor's lock
/// symlink, a swap file, an untracked draft) is read or shipped. A listed
/// path that is not a regular file is an error with what to change.
fn listed_file(source: &Path, line: &str, files: &mut Vec<String>) {
    let metadata = fs::symlink_metadata(source).unwrap_or_else(|e| {
        panic!("plugin-files.txt lists {line}, which cannot be read: {e}; fix the list")
    });
    if metadata.is_dir() {
        panic!("plugin-files.txt lists the directory {line}; name each file in it instead");
    }
    if !metadata.is_file() {
        panic!("plugin-files.txt lists {line}, which is not a regular file; list the file itself");
    }
    println!("cargo:rerun-if-changed={}", source.display());
    files.push(line.to_string());
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
        let relative = Path::new(line);
        if relative.as_os_str().is_empty()
            || relative
                .components()
                .any(|component| !matches!(component, Component::Normal(_)))
        {
            panic!("plugin asset path must stay under repository root: {line}");
        }
        listed_file(&root.join(relative), line, &mut files);
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
