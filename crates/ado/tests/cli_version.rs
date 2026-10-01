use ado_testkit::ado;

// Read from the package so a version bump does not edit this test.
const VERSION_LINE: &str = concat!("ado ", env!("CARGO_PKG_VERSION"), "\n");
const VERSION_JSON_LINE: &str = concat!(
    "{\"ok\":true,\"version\":\"",
    env!("CARGO_PKG_VERSION"),
    "\"}\n"
);

#[test]
fn version_plain_matches_contract() {
    let output = ado().arg("version").output().expect("run `ado version`");

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(String::from_utf8_lossy(&output.stdout), VERSION_LINE);
    assert!(
        output.stderr.is_empty(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn version_json_matches_contract() {
    let output = ado()
        .args(["version", "--json"])
        .output()
        .expect("run `ado version --json`");

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(String::from_utf8_lossy(&output.stdout), VERSION_JSON_LINE);
    assert!(
        output.stderr.is_empty(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn version_json_is_parseable() {
    let output = ado()
        .args(["version", "--json"])
        .output()
        .expect("run `ado version --json`");

    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout is a JSON document");

    assert_eq!(value["ok"], true);
    assert_eq!(value["version"], env!("CARGO_PKG_VERSION"));
}

#[test]
fn version_json_has_no_ansi() {
    let output = ado()
        .args(["version", "--json"])
        .output()
        .expect("run `ado version --json`");

    assert!(
        !output.stdout.contains(&0x1B),
        "stdout contains an ESC byte: {:?}",
        String::from_utf8_lossy(&output.stdout)
    );
}
