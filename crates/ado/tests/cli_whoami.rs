//! End-to-end tests for `ado whoami`, driving the real binary against a
//! temporary home so no test reads or writes the developer's own config.

use std::process::{Command, Output};

use ado_testkit::{TempHome, ado_bin};
use serde_json::{Value, json};

/// The oracle's unauthenticated block with the trailing colour artefact dropped
/// and the stale `ado_cli login` hint corrected to `ado login` (spec §8).
const UNAUTHENTICATED_PLAIN: &str = "\n  Server:       dev.azure.com (cloud)\n  Not authenticated.\n\n  Authenticate with:\n    ado login --method pat --org ORG --pat TOKEN\n    ado login --method device --org ORG\n  Or set environment variables: ADO_ORG + ADO_PAT\n\n";

fn ado(home: &TempHome) -> Command {
    let mut command = Command::new(ado_bin());
    home.apply(&mut command);
    command
        .env_remove("ADO_ORG")
        .env_remove("ADO_PAT")
        .env_remove("ADO_SERVER");
    command
}

fn config_file(home: &TempHome) -> String {
    home.config_dir().join("config.toml").display().to_string()
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn json_result(output: &Output) -> Value {
    serde_json::from_slice::<Value>(&output.stdout).expect("stdout is a JSON document")["result"]
        .clone()
}

fn assert_success(output: &Output) {
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(output));
}

#[test]
fn whoami_json_unauthenticated_matches_oracle() {
    let home = TempHome::new();

    let output = ado(&home)
        .args(["whoami", "--json"])
        .output()
        .expect("run `ado whoami --json`");

    assert_success(&output);
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).expect("stdout is a JSON document"),
        json!({
            "ok": true,
            "result": {
                "server": "dev.azure.com",
                "org": null,
                "method": null,
                "configured": false,
                "authenticated": false,
                "config_file": config_file(&home),
            },
        })
    );
    assert!(output.stderr.is_empty(), "stderr: {}", stderr(&output));
}

#[test]
fn whoami_env_credentials_report_pat() {
    let home = TempHome::new();

    let output = ado(&home)
        .env("ADO_ORG", "myorg")
        .env("ADO_PAT", "pat-token")
        .args(["whoami", "--json"])
        .output()
        .expect("run `ado whoami --json`");

    assert_success(&output);
    let result = json_result(&output);

    assert_eq!(result["org"], "myorg");
    assert_eq!(result["method"], "pat");
    assert_eq!(result["authenticated"], true);
    assert_eq!(result["configured"], false);
    assert_eq!(result["server"], "dev.azure.com");
}

#[test]
fn whoami_config_credentials_report_method() {
    let home = TempHome::new();
    std::fs::write(
        home.config_dir().join("config.toml"),
        "default_org = \"myorg\"\nserver = \"https://ado.example.com\"\n\n[orgs.myorg]\nauth = \"device\"\n",
    )
    .expect("write config.toml");
    std::fs::write(
        home.config_dir().join("credentials.json"),
        r#"{"myorg":{"method":"browser","token":"stored-token"}}"#,
    )
    .expect("write credentials.json");

    let output = ado(&home)
        .args(["whoami", "--json"])
        .output()
        .expect("run `ado whoami --json`");

    assert_success(&output);
    let result = json_result(&output);

    assert_eq!(result["configured"], true);
    assert_eq!(result["org"], "myorg");
    assert_eq!(
        result["method"], "device",
        "the method comes from config.toml, not the credential store"
    );
    assert_eq!(result["server"], "https://ado.example.com");
    assert_eq!(result["authenticated"], true);
    assert_eq!(result["config_file"], config_file(&home));
    assert!(
        !stdout(&output).contains("stored-token"),
        "a stored token reached stdout: {}",
        stdout(&output)
    );
}

#[test]
fn whoami_plain_lines_match_contract() {
    let home = TempHome::new();

    let output = ado(&home).arg("whoami").output().expect("run `ado whoami`");

    assert_success(&output);
    assert_eq!(stdout(&output), UNAUTHENTICATED_PLAIN);
    assert!(output.stderr.is_empty(), "stderr: {}", stderr(&output));
    assert!(
        !output.stdout.contains(&0x1B),
        "stdout contains an ESC byte: {:?}",
        stdout(&output)
    );
    assert!(
        !stdout(&output).contains("ado_cli"),
        "the stale `ado_cli login` hint survived: {}",
        stdout(&output)
    );
}

#[test]
fn pat_never_leaks_to_output() {
    let home = TempHome::new();

    let flag = ado(&home)
        .args(["whoami", "--json", "--verbose", "--pat", "flag-secret"])
        .output()
        .expect("run `ado whoami --json --verbose --pat`");
    assert_success(&flag);
    assert_eq!(json_result(&flag)["method"], "pat");
    assert!(
        !stdout(&flag).contains("flag-secret"),
        "stdout: {}",
        stdout(&flag)
    );
    assert!(
        !stderr(&flag).contains("flag-secret"),
        "stderr: {}",
        stderr(&flag)
    );

    let env = ado(&home)
        .env("ADO_PAT", "env-secret")
        .args(["whoami", "--json", "--verbose"])
        .output()
        .expect("run `ado whoami --json --verbose` with ADO_PAT");
    assert_success(&env);
    assert_eq!(json_result(&env)["method"], "pat");
    assert!(
        !stdout(&env).contains("env-secret"),
        "stdout: {}",
        stdout(&env)
    );
    assert!(
        !stderr(&env).contains("env-secret"),
        "stderr: {}",
        stderr(&env)
    );
}

#[test]
fn whoami_global_after_subcommand_propagates() {
    let home = TempHome::new();

    let output = ado(&home)
        .args([
            "whoami",
            "--org",
            "myorg",
            "--server",
            "https://ado.example.com",
            "--json",
        ])
        .output()
        .expect("run `ado whoami --org myorg --server ... --json`");

    assert_success(&output);
    let result = json_result(&output);

    assert_eq!(result["org"], "myorg");
    assert_eq!(result["server"], "https://ado.example.com");
    assert_eq!(result["authenticated"], true);
}

#[test]
fn whoami_plain_configured_matches_oracle() {
    let home = TempHome::new();
    std::fs::write(
        home.config_dir().join("config.toml"),
        "default_org = \"myorg\"\n\n[orgs.myorg]\nauth = \"pat\"\n",
    )
    .expect("write config.toml");

    let output = ado(&home).arg("whoami").output().expect("run `ado whoami`");

    assert_success(&output);
    assert_eq!(
        stdout(&output),
        format!(
            "\n  Organization: myorg\n  Server:       dev.azure.com (cloud)\n  Auth Method:  pat\n  Config File:  {}\n\n",
            config_file(&home)
        )
    );
    assert!(
        !output.stdout.contains(&0x1B),
        "stdout contains an ESC byte: {:?}",
        stdout(&output)
    );
}
