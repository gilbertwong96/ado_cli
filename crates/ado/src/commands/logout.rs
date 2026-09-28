//! `ado logout` — removes the stored credential for the resolved organization and
//! the `config.toml` entry that references it, ported from
//! `lib/ado_cli/cli/logout.ex` (`run/1`) and `AdoCli.Auth.logout/0` →
//! `AdoCli.ConfigFile.delete/0`.
//!
//! The oracle deletes its whole `~/.ado_cli/config.json` — the single file its
//! token happened to live in — and reports success even when that deletion fails.
//! This build's config is organization-scoped and never holds the token (§7), so
//! logout removes the resolved organization's credential from both store layers
//! and its config entry, keeps every other setting, and reports a failure instead
//! of a false success (inventory D27).

use ado_core::envelope::ok_message;
use ado_core::error::AdoError;

use crate::context::Context;
use crate::output::Report;

/// `run/1`: clear the credential, then report — the envelope is the oracle's
/// `{"ok":true,"message":…}` shape with the message this build can stand behind.
pub fn run(context: &mut Context) -> Result<Report, AdoError> {
    let removed = context.logout()?;
    let message = match removed {
        Some(org) => format!("Logged out. Credentials removed for '{org}'."),
        None => "Logged out. No stored credentials to remove.".to_owned(),
    };
    let envelope = ok_message(&message);

    Ok(context.json_or_report(envelope, || Report::Text(message)))
}

#[cfg(test)]
mod tests {
    use std::fs;

    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    use ado_core::config::{AuthMethod, Config, OrgEntry};
    use ado_core::credentials::{
        CREDENTIALS_FILE, FallbackStore, FileStore, InMemoryStore, SecretStore, Stored,
    };
    use ado_core::env::MapEnv;
    use ado_core::error::ErrorCode;
    use ado_testkit::TempHome;

    use super::*;
    use crate::args::GlobalOpts;

    fn opts(json: bool) -> GlobalOpts {
        GlobalOpts {
            org: None,
            pat: None,
            server: None,
            verbose: false,
            json,
        }
    }

    fn stored(method: AuthMethod, token: &str) -> Stored {
        Stored {
            method,
            token: token.to_owned(),
        }
    }

    fn config_with_org(org: &str) -> Config {
        Config {
            default_org: Some(org.to_owned()),
            orgs: std::collections::BTreeMap::from([(
                org.to_owned(),
                OrgEntry {
                    auth: AuthMethod::Pat,
                },
            )]),
            ..Config::default()
        }
    }

    /// A context whose environment, store and files the test owns: no test reads
    /// the process environment, the OS keychain or the developer's home.
    fn test_context(json: bool, store: impl SecretStore + 'static, home: &TempHome) -> Context {
        Context::for_test(opts(json), MapEnv::new(), store, home)
    }

    /// A read-only directory stands in for the credentials file that cannot be
    /// rewritten; the keychain layer must still be attempted (the Task 2 fix),
    /// and the surviving credential must be reported rather than hidden.
    #[cfg(unix)]
    #[test]
    fn a_failing_file_layer_still_attempts_the_other_layer() {
        let home = TempHome::new();
        let blocked = home.path().join("blocked");
        fs::create_dir_all(&blocked).expect("create the blocked directory");
        let path = blocked.join(CREDENTIALS_FILE);
        let file = FileStore::new(path.clone());
        file.set("myorg", &stored(AuthMethod::Pat, "file-token"))
            .expect("seed the file layer");
        fs::set_permissions(&blocked, fs::Permissions::from_mode(0o555))
            .expect("make the directory read-only");

        if fs::write(blocked.join("probe"), b"probe").is_ok() {
            // Running as root: the mode bits do not apply, so there is nothing to assert.
            fs::remove_file(blocked.join("probe")).expect("remove the probe");
            fs::set_permissions(&blocked, fs::Permissions::from_mode(0o755))
                .expect("restore the directory");
            return;
        }

        let keychain = InMemoryStore::new();
        keychain
            .set("myorg", &stored(AuthMethod::Pat, "keychain-token"))
            .expect("seed the keychain layer");
        let store = FallbackStore::new(keychain.clone(), file.clone());
        let mut context = test_context(true, store, &home).with_config(config_with_org("myorg"));

        let error = run(&mut context).expect_err("the file layer cannot be rewritten");
        fs::set_permissions(&blocked, fs::Permissions::from_mode(0o755))
            .expect("restore the directory");

        assert_eq!(error.code, ErrorCode::ValidationError);
        assert!(
            error.message.contains(CREDENTIALS_FILE),
            "message: {}",
            error.message
        );
        assert_eq!(
            keychain.get("myorg").expect("the keychain is reachable"),
            None,
            "the keychain layer is attempted even though the file layer failed first"
        );
        assert_eq!(
            file.get("myorg").expect("the file layer is readable"),
            Some(stored(AuthMethod::Pat, "file-token")),
            "the failed layer keeps its credential, so the failure is not a lie"
        );
    }

    /// Nothing names an organization: no store call, no config write, and the
    /// message says there was nothing to remove.
    #[test]
    fn a_context_with_no_org_does_not_touch_the_store() {
        let home = TempHome::new();
        let store = InMemoryStore::new();
        let mut context = test_context(true, store.clone(), &home);

        let report = run(&mut context).expect("nothing to remove is a success");

        assert_eq!(
            report,
            Report::Json(serde_json::json!({
                "ok": true,
                "message": "Logged out. No stored credentials to remove.",
            }))
        );
        assert!(
            store.calls().is_empty(),
            "an unresolvable organization never reaches the store: {:?}",
            store.calls()
        );
        assert!(!home.config_dir().join("config.toml").exists());
    }
}
