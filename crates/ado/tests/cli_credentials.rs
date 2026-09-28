//! End-to-end credential tests: no invocation may write a token to stdout or
//! stderr.
//!
//! The client boundary itself is asserted in `crates/ado/src/context.rs` against
//! the mock server, because no command reaches for a client until the read paths
//! land.

use std::process::Output;

use ado_testkit::{TempHome, ado_cmd, stderr_of, stdout_of};

/// The two tokens every invocation below carries: one in `ADO_PAT`, one in
/// `--pat`, so a leak from either source is caught.
const ENV_PAT: &str = "env-secret-pat";
const FLAG_PAT: &str = "flag-secret-pat";

fn run(home: &TempHome, args: &[&str]) -> Output {
    let mut command = ado_cmd();
    home.apply(&mut command);
    command
        .env_remove("ADO_SERVER")
        .env("ADO_ORG", "myorg")
        .env("ADO_PAT", ENV_PAT)
        .args(args);
    command.output().expect("run ado")
}

/// A status command must not reach for a client, so it must not resolve a
/// credential, and with it must not import a legacy install (W1-5, R37).
#[test]
fn a_status_command_never_imports_the_legacy_file() {
    let home = TempHome::new();
    let legacy = home.path().join(".ado_cli").join("config.json");
    std::fs::create_dir_all(legacy.parent().expect("the legacy directory"))
        .expect("create the legacy directory");
    std::fs::write(
        &legacy,
        r#"{"org":"legacyorg","method":"pat","pat":"legacy-pat"}"#,
    )
    .expect("write the legacy file");

    let mut command = ado_cmd();
    home.apply(&mut command);
    command
        .env_remove("ADO_ORG")
        .env_remove("ADO_PAT")
        .env_remove("ADO_SERVER")
        .args(["whoami", "--json"]);
    let output = command.output().expect("run `ado whoami --json`");
    let stdout = stdout_of(&output);

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        stderr_of(&output)
    );
    let result = serde_json::from_str::<serde_json::Value>(&stdout).expect("a JSON document");
    assert_eq!(result["result"]["authenticated"], false);
    assert!(!stdout.contains("legacy-pat"), "stdout: {stdout}");
    assert!(
        !home.config_dir().join("config.toml").exists(),
        "a status command must not import the legacy config file"
    );
}

#[test]
fn no_command_output_contains_the_pat() {
    // `version`, `schema` and `whoami` ignore the token; the last two fail on
    // purpose, so both of the CLI's error paths are covered as well: the JSON
    // envelope on stdout, and the plain line on stderr.
    let commands: [(&[&str], i32, bool); 5] = [
        (&["version"], 0, false),
        (&["whoami", "--json", "--pat", FLAG_PAT], 0, false),
        (&["schema", "--json", "--pat", FLAG_PAT], 0, false),
        (
            &["schema", "no-such-command", "--json", "--pat", FLAG_PAT],
            1,
            false,
        ),
        (
            &["completion", "--shell", "nope", "--pat", FLAG_PAT],
            1,
            true,
        ),
    ];

    for (args, exit, fails_plainly) in commands {
        let home = TempHome::new();
        let output = run(&home, args);
        let stdout = stdout_of(&output);
        let stderr = stderr_of(&output);

        assert_eq!(output.status.code(), Some(exit), "{args:?}");
        if fails_plainly {
            assert!(!stderr.is_empty(), "{args:?}: the failure line is missing");
        } else {
            assert!(stderr.is_empty(), "{args:?}: unexpected stderr: {stderr}");
        }

        for (stream, text) in [("stdout", &stdout), ("stderr", &stderr)] {
            assert!(
                !text.contains(ENV_PAT),
                "{args:?}: ADO_PAT reached {stream}: {text}"
            );
            assert!(
                !text.contains(FLAG_PAT),
                "{args:?}: --pat reached {stream}: {text}"
            );
        }
    }
}
