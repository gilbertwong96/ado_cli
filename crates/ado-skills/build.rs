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
    let assets = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"))
        .join("assets");
    let mut files = Vec::new();
    collect(&assets, &assets, &mut files);
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

fn collect(root: &Path, dir: &Path, files: &mut Vec<String>) {
    let entries = fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("read {}: {error}", dir.display()));

    for entry in entries {
        let entry = entry.expect("a directory entry");
        let path = entry.path();

        if path.is_dir() {
            collect(root, &path, files);
        } else {
            files.push(
                path.strip_prefix(root)
                    .expect("a path under assets/")
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }
}
