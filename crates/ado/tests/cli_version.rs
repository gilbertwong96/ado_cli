use ado_testkit::ado;

const VERSION_LINE: &str = "ado 1.0.0-rc.0\n";
const VERSION_JSON_LINE: &str = "{\"ok\":true,\"version\":\"1.0.0-rc.0\"}\n";

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
    assert_eq!(value["version"], "1.0.0-rc.0");
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
