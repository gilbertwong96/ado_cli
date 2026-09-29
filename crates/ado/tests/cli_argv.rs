use ado_testkit::{ado, stderr_of, stdout_of};
use serde_json::{Value, json};

const VERSION_LINE: &str = "ado 1.0.0-rc.0\n";

#[test]
fn version_flag_short_circuits() {
    let output = ado()
        .arg("--version")
        .output()
        .expect("run `ado --version`");

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(stdout_of(&output), VERSION_LINE);
    assert!(
        !stdout_of(&output).contains("Usage"),
        "help text leaked onto stdout: {}",
        stdout_of(&output)
    );
    assert!(
        stderr_of(&output).is_empty(),
        "stderr: {}",
        stderr_of(&output)
    );
}

#[test]
fn capital_v_is_not_a_version_flag() {
    let output = ado().arg("-V").output().expect("run `ado -V`");

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr_of(&output).contains("-V"),
        "stderr does not mention the unknown option: {}",
        stderr_of(&output)
    );
    assert!(
        !stdout_of(&output).contains(VERSION_LINE.trim()),
        "`-V` printed the version line: {}",
        stdout_of(&output)
    );
}

#[test]
fn global_opts_parse_before_and_after() {
    let before = ado()
        .args(["--org", "example", "version"])
        .output()
        .expect("run `ado --org example version`");
    assert_eq!(before.status.code(), Some(0));
    assert_eq!(stdout_of(&before), VERSION_LINE);

    let after = ado()
        .args(["version", "--org", "example"])
        .output()
        .expect("run `ado version --org example`");
    assert_eq!(after.status.code(), Some(0));
    assert_eq!(stdout_of(&after), VERSION_LINE);
}

#[test]
fn server_short_is_s() {
    let output = ado()
        .args(["-s", "https://example.test", "version"])
        .output()
        .expect("run `ado -s https://example.test version`");

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        stderr_of(&output)
    );
    assert_eq!(stdout_of(&output), VERSION_LINE);
}

/// D6: `-h`/`--help` is a display request, not a usage error — help on stdout,
/// exit 0. It had no test and no harness case; `bare_ado_*` above is the
/// missing-subcommand help (D13).
#[test]
fn help_flags_are_display_requests() {
    for flag in ["-h", "--help"] {
        let output = ado().arg(flag).output().expect("run `ado --help`");

        assert_eq!(output.status.code(), Some(0), "{flag}");
        assert!(
            stdout_of(&output).contains("Usage:"),
            "{flag}: help went to stdout: {}",
            stdout_of(&output)
        );
        assert!(
            stderr_of(&output).is_empty(),
            "{flag}: stderr: {}",
            stderr_of(&output)
        );
    }
}

#[test]
fn bare_ado_is_a_missing_subcommand() {
    let output = ado().output().expect("run bare `ado`");

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stdout_of(&output).contains("Usage:"),
        "bare `ado` did not print root help: {}",
        stdout_of(&output)
    );
    assert_eq!(
        stderr_of(&output),
        "[Validation error] missing sub-command\n"
    );
}

#[test]
fn bare_ado_json_is_the_error_envelope() {
    let output = ado().arg("--json").output().expect("run bare `ado --json`");

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr_of(&output).is_empty(),
        "stderr: {}",
        stderr_of(&output)
    );

    let envelope: Value =
        serde_json::from_slice(&output.stdout).expect("stdout is one JSON document");
    assert_eq!(envelope["ok"], json!(false));
    assert_eq!(envelope["error"]["code"], json!("validation_error"));
    assert_eq!(envelope["error"]["message"], json!("missing sub-command"));
}

#[cfg(unix)]
#[test]
fn invalid_utf8_arg_errors_without_panic() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let output = ado()
        .arg(OsString::from_vec(vec![0xff]))
        .output()
        .expect("run `ado <invalid-utf8>`");

    assert_eq!(output.status.code(), Some(1));
    assert!(
        !stderr_of(&output).contains("panicked"),
        "stderr: {}",
        stderr_of(&output)
    );
    assert!(
        stderr_of(&output).contains("invalid UTF-8"),
        "stderr: {}",
        stderr_of(&output)
    );
    assert!(
        stdout_of(&output).is_empty(),
        "stdout: {}",
        stdout_of(&output)
    );
}
