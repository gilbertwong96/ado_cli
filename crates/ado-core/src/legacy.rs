//! One-time import of the Elixir CLI's `~/.ado_cli/config.json` (spec §7), so an
//! existing install keeps working without a re-auth.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::config::{AuthMethod, Config, OrgEntry, save_at};
use crate::credentials::{SecretStore, Stored};
use crate::env::non_empty;
use crate::error::AdoError;

/// The legacy config file, relative to the home directory.
pub const LEGACY_RELATIVE_PATH: &str = ".ado_cli/config.json";

/// The credential one legacy install held.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyCreds {
    pub org: String,
    pub server: Option<String>,
    pub method: AuthMethod,
    pub token: String,
}

#[derive(Deserialize)]
struct LegacyFile {
    #[serde(default)]
    org: Option<String>,
    #[serde(default)]
    server: Option<String>,
    #[serde(default)]
    method: Option<String>,
    #[serde(default)]
    pat: Option<String>,
    #[serde(default)]
    token: Option<String>,
}

/// `~/.ado_cli/config.json`.
pub fn legacy_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(LEGACY_RELATIVE_PATH))
}

/// Reads the legacy file. A missing, unreadable or malformed file reads as absent.
pub fn read_legacy() -> Result<Option<LegacyCreds>, AdoError> {
    Ok(legacy_path().as_deref().and_then(read_legacy_at))
}

/// Imports the legacy credential into `config` and `store` the first time it
/// runs, and never again once the new config file exists. `config_file` is where
/// the new config belongs — its absence is the "not imported yet" marker — and
/// `legacy_file` is the Elixir CLI's `config.json`. Both come from the caller
/// (`Context` in the binary), so the import reads and writes exactly the paths
/// the run already resolved.
///
/// A blank value is not a value (`env::non_empty`), so a legacy file whose org,
/// token or server is whitespace-only reads as absent where the first version of
/// this function accepted it (ruling W1-R3).
pub fn import_once(
    config: &mut Config,
    store: &dyn SecretStore,
    config_file: Option<&Path>,
    legacy_file: Option<&Path>,
) -> Result<bool, AdoError> {
    let Some(target) = config_file.filter(|path| !path.exists()) else {
        return Ok(false);
    };

    let Some(legacy) = legacy_file.and_then(read_legacy_at) else {
        return Ok(false);
    };

    config.default_org = Some(legacy.org.clone());
    config.server = legacy.server.clone().or_else(|| config.server.clone());
    config.orgs.insert(
        legacy.org.clone(),
        OrgEntry {
            auth: legacy.method,
        },
    );

    // The credential goes in first: if it fails, no marker is left behind and the
    // next run imports again. Storing twice is harmless, dropping the token is not.
    store.set(
        &legacy.org,
        &Stored {
            method: legacy.method,
            token: legacy.token,
        },
    )?;
    save_at(target, config)?;

    Ok(true)
}

fn read_legacy_at(path: &Path) -> Option<LegacyCreds> {
    let text = fs::read_to_string(path).ok()?;
    let file: LegacyFile = serde_json::from_str(&text).ok()?;

    let org = file.org.filter(|org| non_empty(org))?;
    let method: AuthMethod = legacy_method(file.method.as_deref())?;
    let token = file.pat.or(file.token).filter(|token| non_empty(token))?;

    Some(LegacyCreds {
        org,
        server: file.server.filter(|server| non_empty(server)),
        method,
        token,
    })
}

fn legacy_method(method: Option<&str>) -> Option<AuthMethod> {
    match method? {
        "pat" => Some(AuthMethod::Pat),
        "device_code" | "device" => Some(AuthMethod::Device),
        "browser" => Some(AuthMethod::Browser),
        // `az_cli` and unknown methods kept no token the new config can express.
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::config::load_at;
    use crate::credentials::InMemoryStore;
    use crate::test_support::TempDir;

    fn legacy_file(dir: &TempDir, contents: &str) -> PathBuf {
        let path = dir.path().join(".ado_cli").join("config.json");
        fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        fs::write(&path, contents).expect("write legacy file");
        path
    }

    #[test]
    fn legacy_import_reads_org_server_method_pat() {
        let dir = TempDir::new("legacy-import");
        let legacy = legacy_file(
            &dir,
            r#"{"org":"myorg","server":"https://ado.example.com","method":"pat","pat":"legacy-pat"}"#,
        );
        let config_file = dir.path().join("ado").join("config.toml");
        let store = InMemoryStore::new();
        let mut config = Config::default();

        let imported = import_once(&mut config, &store, Some(&config_file), Some(&legacy));

        assert!(
            imported.expect("import"),
            "the legacy file must be imported"
        );
        let saved = load_at(&config_file)
            .expect("load")
            .expect("the new config is written");
        assert_eq!(saved.default_org.as_deref(), Some("myorg"));
        assert_eq!(saved.server.as_deref(), Some("https://ado.example.com"));
        assert_eq!(
            saved.orgs.get("myorg"),
            Some(&OrgEntry {
                auth: AuthMethod::Pat
            })
        );
        assert_eq!(
            store.get("myorg").expect("get"),
            Some(Stored {
                method: AuthMethod::Pat,
                token: "legacy-pat".to_owned()
            })
        );

        let text = fs::read_to_string(&config_file).expect("read config");
        assert!(
            !text.contains("legacy-pat"),
            "config.toml must never hold a token: {text}"
        );
    }

    #[test]
    fn legacy_import_reads_token_key_and_device_method() {
        let dir = TempDir::new("legacy-token-key");
        let legacy = legacy_file(
            &dir,
            r#"{"org":"myorg","method":"device_code","token":"oauth-token"}"#,
        );
        let config_file = dir.path().join("ado").join("config.toml");
        let store = InMemoryStore::new();

        let imported = import_once(
            &mut Config::default(),
            &store,
            Some(&config_file),
            Some(&legacy),
        )
        .expect("import");

        assert!(imported);
        let saved = load_at(&config_file)
            .expect("load")
            .expect("config written");
        assert_eq!(saved.server, None);
        assert_eq!(
            saved.orgs.get("myorg"),
            Some(&OrgEntry {
                auth: AuthMethod::Device
            })
        );
        assert_eq!(
            store.get("myorg").expect("get"),
            Some(Stored {
                method: AuthMethod::Device,
                token: "oauth-token".to_owned()
            })
        );
    }

    #[test]
    fn legacy_import_skips_malformed_json() {
        let dir = TempDir::new("legacy-malformed");
        let config_file = dir.path().join("ado").join("config.toml");
        let store = InMemoryStore::new();

        let cases = [
            ("truncated", r#"{"org":"myorg","#),
            ("not-json", "not json at all"),
            ("wrong-shape", "[]"),
            ("no-token", r#"{"org":"myorg","method":"pat"}"#),
            (
                "unknown-method",
                r#"{"org":"myorg","method":"az_cli","token":"t"}"#,
            ),
        ];

        for (label, contents) in cases {
            let legacy = legacy_file(&dir, contents);

            let imported = import_once(
                &mut Config::default(),
                &store,
                Some(&config_file),
                Some(&legacy),
            )
            .expect("a malformed legacy file is not an error");

            assert!(!imported, "{label}");
        }

        assert!(
            !config_file.exists(),
            "a skipped import must not write a config"
        );
        assert!(
            store.calls().is_empty(),
            "a skipped import must not touch the store"
        );
    }

    #[test]
    fn legacy_import_reads_whitespace_only_values_as_absent() {
        let dir = TempDir::new("legacy-blank");
        let config_file = dir.path().join("ado").join("config.toml");
        let store = InMemoryStore::new();

        // W1-R3: one trim-based blank-means-unset predicate everywhere, so a
        // whitespace-only org or token is not a value and nothing imports.
        for (label, contents) in [
            ("org", r#"{"org":"  ","method":"pat","pat":"legacy-pat"}"#),
            ("token", r#"{"org":"myorg","method":"pat","pat":"  "}"#),
        ] {
            let legacy = legacy_file(&dir, contents);

            let imported = import_once(
                &mut Config::default(),
                &store,
                Some(&config_file),
                Some(&legacy),
            )
            .expect("a blank legacy value is not an error");

            assert!(!imported, "{label}");
            assert!(
                !config_file.exists(),
                "{label}: a skipped import writes nothing"
            );
        }

        // A whitespace-only server is dropped, while the credential still imports.
        let legacy = legacy_file(
            &dir,
            r#"{"org":"myorg","server":"  ","method":"pat","pat":"legacy-pat"}"#,
        );

        let imported = import_once(
            &mut Config::default(),
            &store,
            Some(&config_file),
            Some(&legacy),
        )
        .expect("import");

        assert!(imported);
        assert_eq!(
            load_at(&config_file)
                .expect("load")
                .expect("written")
                .server,
            None
        );
    }

    #[test]
    fn legacy_import_stores_the_credential_before_the_config_marker() {
        let dir = TempDir::new("legacy-failing-store");
        let legacy = legacy_file(&dir, r#"{"org":"myorg","method":"pat","pat":"legacy-pat"}"#);
        let config_file = dir.path().join("ado").join("config.toml");

        let imported = import_once(
            &mut Config::default(),
            &InMemoryStore::unavailable(),
            Some(&config_file),
            Some(&legacy),
        );

        assert!(imported.is_err(), "an unusable store must fail the import");
        assert!(
            !config_file.exists(),
            "a failed import must not leave the config marker, or the token is dropped forever"
        );
    }

    #[test]
    fn legacy_import_runs_once() {
        let dir = TempDir::new("legacy-once");
        let legacy = legacy_file(&dir, r#"{"org":"myorg","method":"pat","pat":"legacy-pat"}"#);
        let config_file = dir.path().join("ado").join("config.toml");
        let store = InMemoryStore::new();
        let before = fs::read_to_string(&legacy).expect("read legacy");

        let first = import_once(
            &mut Config::default(),
            &store,
            Some(&config_file),
            Some(&legacy),
        )
        .expect("first import");
        let second = import_once(
            &mut Config::default(),
            &store,
            Some(&config_file),
            Some(&legacy),
        )
        .expect("second import");

        assert!(first);
        assert!(!second, "the import runs once the new config exists");
        assert_eq!(
            fs::read_to_string(&legacy).expect("read legacy"),
            before,
            "the legacy file is left untouched"
        );

        fs::write(
            &legacy,
            r#"{"org":"otherorg","method":"pat","pat":"other-pat"}"#,
        )
        .expect("rewrite legacy");
        let third = import_once(
            &mut Config::default(),
            &store,
            Some(&config_file),
            Some(&legacy),
        )
        .expect("third import");

        assert!(!third, "a later legacy edit is never imported");
        assert_eq!(store.calls().len(), 1, "the credential is stored once");
        assert_eq!(
            load_at(&config_file)
                .expect("load")
                .expect("config written")
                .default_org
                .as_deref(),
            Some("myorg")
        );
    }
}
