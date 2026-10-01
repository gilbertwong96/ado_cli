//! Embeds `assets/` as a `(relative path, content)` table, the way the frozen
//! `AdoCli.Skills` module embedded `priv/skills/` at compile time: every regular
//! file under the directory is a skill file, and adding one needs no code change.
//!
//! The table's keys always use `/`, so a skill's reference paths are the same on
//! every platform (`Path.relative_to/2` hands the Elixir backslashes on Windows,
//! where its own `split_arg/1` then looks up a `/`-joined key and misses — this
//! build keeps one spelling).

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let assets =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR")).join("assets");
    let mut files = Vec::new();

    // The frozen module filters `File.ls!/1` through `File.dir?/1`, so a top-level
    // entry is a skill only when it is a directory; a stray file at the root is
    // ignored, not embedded.
    for entry in
        fs::read_dir(&assets).unwrap_or_else(|error| panic!("read {}: {error}", assets.display()))
    {
        let entry = entry.expect("a directory entry");
        let path = entry.path();

        if entry_type(&entry).is_dir() {
            collect(&assets, &path, &mut files);
        }
    }

    files.sort();

    println!("cargo:rerun-if-changed=assets");

    let mut generated = String::from("pub static EMBEDDED_FILES: &[(&str, &str)] = &[\n");
    for relative in &files {
        let absolute = assets.join(relative);
        println!("cargo:rerun-if-changed={}", absolute.display());
        generated.push_str(&format!(
            "    ({relative:?}, include_str!({:?})),\n",
            absolute.to_str().expect("a UTF-8 asset path")
        ));
    }
    generated.push_str("];\n");

    let out = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR")).join("embedded_assets.rs");
    fs::write(&out, generated).expect("write the embedded asset table");
}

/// A directory entry's own type, not its target's: a directory symlink would
/// otherwise embed its target a second time under the link's key (and a cycle
/// would recurse to the OS path limit), so the assets directory is files and real
/// directories only.
fn entry_type(entry: &fs::DirEntry) -> fs::FileType {
    let path = entry.path();
    let file_type = entry
        .file_type()
        .unwrap_or_else(|error| panic!("stat {}: {error}", path.display()));

    assert!(
        !file_type.is_symlink(),
        "assets/ must not contain symlinks ({} is one)",
        path.display()
    );

    file_type
}

fn collect(root: &Path, dir: &Path, files: &mut Vec<String>) {
    let entries =
        fs::read_dir(dir).unwrap_or_else(|error| panic!("read {}: {error}", dir.display()));

    for entry in entries {
        let entry = entry.expect("a directory entry");
        let path = entry.path();

        if entry_type(&entry).is_dir() {
            collect(root, &path, files);
        } else {
            // The UTF-8 check runs before the key is built, so a non-UTF-8 name
            // fails here with this message rather than with rustc's
            // `couldn't read` on a lossy path that does not exist.
            let relative = path
                .strip_prefix(root)
                .expect("a path under assets/")
                .to_str()
                .expect("a UTF-8 asset path")
                .replace('\\', "/");
            files.push(relative);
        }
    }
}
