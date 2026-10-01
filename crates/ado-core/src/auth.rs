//! Request headers for a resolved credential, and the credential a login stores.

use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::json;

use crate::config::{AuthMethod, Config, OrgEntry};
use crate::credentials::{Credentials, SecretStore, Stored};
use crate::error::{AdoError, ErrorCode};

pub mod browser;
pub mod device_code;
pub mod identity;

/// PATs are sent as HTTP Basic with an empty username — `Basic base64(":PAT")` —
/// and OAuth tokens as Bearer, matching the Elixir CLI.
pub fn auth_header(credentials: &Credentials) -> (String, String) {
    let value = match credentials.method {
        AuthMethod::Pat => format!(
            "Basic {}",
            STANDARD.encode(format!(":{}", credentials.token))
        ),
        AuthMethod::Device | AuthMethod::Browser => format!("Bearer {}", credentials.token),
    };

    ("Authorization".to_owned(), value)
}

/// Records a completed login: the credential in `store` under `org`, and the
/// organization and method in `config` — never the token, which is what keeps the
/// config file secret-free (§7). `ado login --method pat` passes [`AuthMethod::Pat`]
/// and `--method device` [`AuthMethod::Device`]; both perform the same write.
///
/// The credential goes first and the caller writes `config` afterwards: a failed
/// config write then leaves an unreferenced credential (recoverable by naming the
/// organization again), where a config that recorded a login with no credential
/// behind it would report an authenticated setup that cannot make a request.
pub fn save_credential(
    org: &str,
    method: AuthMethod,
    token: &str,
    store: &dyn SecretStore,
    config: &mut Config,
) -> Result<(), AdoError> {
    store.set(
        org,
        &Stored {
            method,
            token: token.to_owned(),
        },
    )?;

    config.default_org = Some(org.to_owned());
    config
        .orgs
        .insert(org.to_owned(), OrgEntry { auth: method });

    Ok(())
}

/// `--org is required for method='<method>' (or set ADO_ORG env var)` and its
/// `details` pair, captured byte-for-byte from the frozen CLI. Every login method
/// needs an organization in this build: `pat` by the oracle's own guard, `device`
/// because our credential store is keyed by one (D26), and `browser` when the
/// freshly exchanged token's account resolves to no single organization (D26's
/// extension — the oracle stores an unkeyed credential there).
pub fn org_required(method: &str) -> AdoError {
    AdoError {
        code: ErrorCode::ValidationError,
        status: None,
        message: format!("--org is required for method='{method}' (or set ADO_ORG env var)"),
        details: Some(json!({"option": "--org", "env_var": "ADO_ORG"})),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credentials::InMemoryStore;

    fn credentials(method: AuthMethod, token: &str) -> Credentials {
        Credentials {
            org: "myorg".to_owned(),
            method,
            token: token.to_owned(),
        }
    }

    #[test]
    fn pat_header_is_basic_with_colon_prefix() {
        let (name, value) = auth_header(&credentials(AuthMethod::Pat, "abc"));

        assert_eq!(name, "Authorization");
        assert_eq!(value, "Basic OmFiYw==");
        assert!(!value.contains("abc"), "the token must not appear raw");
    }

    #[test]
    fn oauth_tokens_are_sent_as_bearer() {
        for method in [AuthMethod::Device, AuthMethod::Browser] {
            let (name, value) = auth_header(&credentials(method, "oauth-token"));

            assert_eq!(name, "Authorization");
            assert_eq!(value, "Bearer oauth-token");
        }
    }

    /// The login write: the credential in the store, the organization and method in
    /// the config, and the token in neither the config's fields nor its rendering.
    #[test]
    fn save_credential_stores_the_token_and_records_the_method() {
        let store = InMemoryStore::new();
        let mut config = Config::default();

        save_credential("myorg", AuthMethod::Pat, "pat-token", &store, &mut config).expect("save");

        assert_eq!(
            store.get("myorg").expect("get"),
            Some(Stored {
                method: AuthMethod::Pat,
                token: "pat-token".to_owned(),
            })
        );
        assert_eq!(config.default_org.as_deref(), Some("myorg"));
        assert_eq!(
            config.orgs.get("myorg"),
            Some(&OrgEntry {
                auth: AuthMethod::Pat,
            })
        );
        assert!(
            !toml::to_string_pretty(&config)
                .expect("render")
                .contains("pat-token"),
            "the token must not be renderable into the config file: {config:?}"
        );
    }

    #[test]
    fn save_credential_keeps_the_settings_it_did_not_touch() {
        let store = InMemoryStore::new();
        let mut config = Config {
            server: Some("https://ado.example.com".to_owned()),
            orgs: std::collections::BTreeMap::from([(
                "other".to_owned(),
                OrgEntry {
                    auth: AuthMethod::Browser,
                },
            )]),
            ..Config::default()
        };

        save_credential(
            "myorg",
            AuthMethod::Device,
            "oauth-token",
            &store,
            &mut config,
        )
        .expect("save");

        assert_eq!(config.server.as_deref(), Some("https://ado.example.com"));
        assert_eq!(config.orgs.len(), 2);
        assert_eq!(
            config.orgs.get("other").map(|entry| entry.auth),
            Some(AuthMethod::Browser)
        );
    }

    /// A failed store write is the only failure this function has, and it must leave
    /// the config untouched: no login is recorded without a credential behind it.
    #[test]
    fn save_credential_leaves_the_config_alone_when_the_store_fails() {
        let store = InMemoryStore::unavailable();
        let mut config = Config::default();

        let error = save_credential("myorg", AuthMethod::Pat, "pat-token", &store, &mut config)
            .expect_err("the store is unavailable");

        assert_eq!(error.code, ErrorCode::ValidationError);
        assert_eq!(config, Config::default());
    }
}
