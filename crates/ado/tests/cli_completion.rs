//! Tests for `ado completion`: the raw scripts, the positional shell argument,
//! and the `-w` path — end-to-end against the binary.

use std::fs;
use std::path::PathBuf;
use std::process::Output;
use std::sync::atomic::{AtomicU32, Ordering};

use assert_cmd::Command;
use serde_json::{Value, json};

const SHELLS: [&str; 4] = ["bash", "zsh", "fish", "powershell"];
const UNKNOWN_SHELL_MESSAGE: &str =
    "Unknown shell 'nope'. Must be one of: bash, zsh, fish, powershell.";

fn ado() -> Command {
    Command::cargo_bin("ado").expect("the ado binary is built")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn completion(shell: &str) -> Output {
    ado()
        .args(["completion", shell])
        .output()
        .unwrap_or_else(|_| panic!("run `ado completion {shell}`"))
}

fn temp_script_path() -> PathBuf {
    std::env::temp_dir().join(format!("ado-completion-{}.zsh", unique_suffix()))
}

fn missing_script_path() -> PathBuf {
    std::env::temp_dir()
        .join(format!("ado-completion-missing-{}", unique_suffix()))
        .join("_ado")
}

fn unique_suffix() -> String {
    static COUNTER: AtomicU32 = AtomicU32::new(0);

    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);

    format!("{}-{unique}", std::process::id())
}

#[test]
fn completion_defaults_to_bash() {
    let default = ado()
        .arg("completion")
        .output()
        .expect("run `ado completion`");

    assert_eq!(
        default.status.code(),
        Some(0),
        "stderr: {}",
        stderr(&default)
    );
    assert_eq!(default.stdout, completion("bash").stdout);
    assert!(!default.stdout.is_empty());
    assert!(default.stderr.is_empty(), "stderr: {}", stderr(&default));
}

#[test]
fn completion_emits_raw_scripts() {
    for shell in SHELLS {
        let output = completion(shell);

        assert_eq!(
            output.status.code(),
            Some(0),
            "shell {shell}: stderr: {}",
            stderr(&output)
        );
        assert!(!output.stdout.is_empty(), "shell {shell}");
        assert!(
            !stdout(&output).starts_with('{'),
            "shell {shell}: stdout begins with a JSON envelope: {:?}",
            stdout(&output)
        );
        assert!(
            output.stderr.is_empty(),
            "shell {shell}: stderr: {}",
            stderr(&output)
        );
    }
}

#[test]
fn completion_json_flag_keeps_the_script_raw() {
    let output = ado()
        .args(["completion", "bash", "--json"])
        .output()
        .expect("run `ado completion bash --json`");

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert_eq!(output.stdout, completion("bash").stdout);
    assert!(output.stderr.is_empty(), "stderr: {}", stderr(&output));
}

#[test]
fn completion_zsh_and_fish_mention_ado() {
    let zsh = completion("zsh");
    let fish = completion("fish");

    assert_eq!(zsh.status.code(), Some(0), "stderr: {}", stderr(&zsh));
    assert_eq!(fish.status.code(), Some(0), "stderr: {}", stderr(&fish));
    assert!(
        stdout(&zsh).contains("#compdef ado"),
        "zsh output: {:?}",
        stdout(&zsh)
    );
    assert!(
        stdout(&fish).contains("complete -c ado"),
        "fish output: {:?}",
        stdout(&fish)
    );
}

#[test]
fn completion_scripts_carry_the_current_invocation() {
    for shell in SHELLS {
        let text = stdout(&completion(shell));

        assert!(
            text.contains(&format!("ado completion {shell}")),
            "shell {shell} does not name its invocation: {text:?}"
        );
        assert!(
            !text.contains("completion -s"),
            "shell {shell} carries the stale invocation: {text:?}"
        );
    }

    assert!(
        stdout(&completion("zsh")).starts_with("#compdef ado\n"),
        "zsh's #compdef tag must stay on the first line"
    );
}

#[test]
fn completion_unknown_shell_is_validation_error_under_json() {
    let output = ado()
        .args(["completion", "nope", "--json"])
        .output()
        .expect("run `ado completion nope --json`");

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).expect("stdout is a JSON document"),
        json!({
            "ok": false,
            "error": {
                "code": "validation_error",
                "message": UNKNOWN_SHELL_MESSAGE,
            },
        })
    );
    assert!(output.stderr.is_empty(), "stderr: {}", stderr(&output));
    assert!(
        !output.stdout.contains(&0x1B),
        "stdout contains an ESC byte: {:?}",
        stdout(&output)
    );
}

#[test]
fn completion_unknown_shell_plain_message() {
    let output = ado()
        .args(["completion", "nope"])
        .output()
        .expect("run `ado completion nope`");

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        stderr(&output),
        format!("[Validation error] {UNKNOWN_SHELL_MESSAGE}\n")
    );
    assert!(output.stdout.is_empty(), "stdout: {}", stdout(&output));
}

#[test]
fn completion_write_to_file_writes_and_prints_nothing() {
    let expected = stdout(&completion("zsh"));

    for flag in ["-w", "--write-to-file"] {
        let path = temp_script_path();
        let output = ado()
            .args(["completion", "zsh", flag])
            .arg(&path)
            .output()
            .unwrap_or_else(|_| panic!("run `ado completion zsh {flag} PATH`"));

        assert_eq!(
            output.status.code(),
            Some(0),
            "flag {flag}: stderr: {}",
            stderr(&output)
        );
        assert!(
            output.stdout.is_empty(),
            "flag {flag}: stdout: {}",
            stdout(&output)
        );
        assert!(
            output.stderr.is_empty(),
            "flag {flag}: stderr: {}",
            stderr(&output)
        );
        assert_eq!(
            fs::read_to_string(&path).expect("the written script"),
            expected,
            "flag {flag}"
        );

        let _ = fs::remove_file(&path);
    }
}

#[test]
fn completion_unwritable_path_is_a_validation_error() {
    let path = missing_script_path();
    let output = ado()
        .args(["completion", "bash", "-w"])
        .arg(&path)
        .output()
        .expect("run `ado completion bash -w MISSING/_ado`");

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty(), "stdout: {}", stdout(&output));
    assert!(
        stderr(&output).starts_with("[Validation error] Could not write the completion script to "),
        "stderr: {}",
        stderr(&output)
    );
    assert!(!path.exists(), "nothing should be written");
}

#[test]
fn completion_covers_wave_zero_commands() {
    for shell in SHELLS {
        let output = completion(shell);
        let text = stdout(&output);

        assert_eq!(
            output.status.code(),
            Some(0),
            "shell {shell}: stderr: {}",
            stderr(&output)
        );

        for command in ["version", "whoami", "schema"] {
            assert!(
                text.contains(command),
                "shell {shell} does not mention {command}: {text:?}"
            );
        }
    }
}
