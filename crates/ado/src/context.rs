//! The per-invocation context: the globals as parsed, the environment they sit in
//! front of, the config file, the credential store, and the client every network
//! command shares.
//!
//! Loading is deliberately read-only (ruling R37): it never opens the credential
//! store and never imports the legacy config, so `whoami`, `schema` and
//! `completion` cannot trigger a keychain prompt. The auth method lives in
//! `config.toml`; the token does not. Credential resolution, and with it the
//! one-time legacy import, happens only when a command asks for the client.

use std::path::PathBuf;

use ado_core::client::Client;
use ado_core::config::{self, AuthMethod, Config};
use ado_core::credentials::{self, Credentials, SecretStore, default_store};
use ado_core::env::{ENV_ORG, ENV_PAT, ENV_SERVER, EnvSource, FlagEnv, ProcessEnv, non_empty};
use ado_core::error::{AdoError, ErrorCode};
use ado_core::legacy;
use serde_json::Value;

use crate::args::GlobalOpts;
use crate::output::Report;

/// The cloud server, used when neither the flags, `ADO_SERVER` nor the config
/// names one.
pub const DEFAULT_SERVER: &str = "dev.azure.com";

/// The authentication status a command reports without touching a secret.
#[derive(Debug, PartialEq)]
pub struct AuthStatus {
    pub configured: bool,
    pub org: Option<String>,
    pub server: String,
    pub method: Option<String>,
    pub authenticated: bool,
    pub config_file: Option<PathBuf>,
}

/// The per-invocation state: what the command line said, the environment it said
/// it in, the config file, and the credential store the run may need.
pub struct Context {
    /// The globals exactly as parsed — the flags that outrank the environment.
    opts: GlobalOpts,
    /// The environment the flags sit in front of: the process environment in
    /// production, a fixed map in tests.
    base_env: Box<dyn EnvSource>,
    /// The loaded config file; `None` when it is missing or malformed.
    config: Option<Config>,
    /// Where the token lives: the OS keychain, with the credentials file behind it.
    store: Box<dyn SecretStore>,
    /// `<config dir>/ado/config.toml`: what `whoami` reports, and the marker that
    /// the one-time legacy import has already run.
    config_file: Option<PathBuf>,
    /// `~/.ado_cli/config.json`: the file the one-time import reads.
    legacy_file: Option<PathBuf>,
    /// The client, built on first use and reused for the rest of the run.
    client: Option<Client>,
}

impl Context {
    /// Reads the config file and captures the process environment, so no consumer
    /// can see `ADO_*` without the flags that outrank it (spec §6.6).
    pub fn load(opts: GlobalOpts) -> Context {
        Context {
            opts,
            base_env: Box::new(ProcessEnv),
            config: config::load().unwrap_or_default(),
            store: Box::new(default_store()),
            config_file: config::config_path(),
            legacy_file: legacy::legacy_path(),
            client: None,
        }
    }

    /// Whether the invocation asked for JSON output.
    pub fn json(&self) -> bool {
        self.opts.json
    }

    /// The one fork between a command's `--json` envelope and its human report:
    /// under `--json` the envelope is the whole answer and `human` is never
    /// called, so no table can reach the JSON path (W1-3, spec §6.1).
    pub fn json_or_report(&self, envelope: Value, human: impl FnOnce() -> Report) -> Report {
        if self.json() {
            Report::Json(envelope)
        } else {
            human()
        }
    }

    /// The client for the rest of the run. The first call resolves the credential,
    /// builds the client and caches it, so one invocation resolves once, reads the
    /// store once, and imports a legacy install at most once (W1-4, W1-5).
    pub fn client(&mut self) -> Result<&Client, AdoError> {
        if self.client.is_none() {
            let credentials = self.credentials()?;
            let env = self.env();
            let client = Client::from_env(&credentials, &env)?;

            self.client = Some(client);
        }

        Ok(self.client.as_ref().expect("the client was just built"))
    }

    /// The status `whoami` reports: a config file that loads sets `configured`,
    /// an organization in the flags, environment or config sets `org`, and the
    /// method is the org's `config.toml` entry, else `pat` when a runtime PAT is
    /// present — never the token itself. `config_file` is `None` only when the OS
    /// cannot resolve a config directory: on macOS `dirs` falls back to the passwd
    /// database when `HOME` is unset, so a normal user always resolves a path.
    pub fn auth_status(&self) -> AuthStatus {
        let env = self.env();
        let configured = self.config.is_some();
        let org = present(env.get(ENV_ORG)).or_else(|| self.default_org());
        let server = present(env.get(ENV_SERVER))
            .or_else(|| {
                self.config
                    .as_ref()
                    .and_then(|config| present(config.server.clone()))
            })
            .unwrap_or_else(|| DEFAULT_SERVER.to_owned());
        let method = self
            .stored_method(org.as_deref())
            .or_else(|| present(env.get(ENV_PAT)).map(|_| AuthMethod::Pat.as_str().to_owned()));

        AuthStatus {
            configured,
            authenticated: configured || org.is_some(),
            org,
            server,
            method,
            config_file: self.config_file.clone(),
        }
    }

    fn default_org(&self) -> Option<String> {
        self.config
            .as_ref()
            .and_then(|config| present(config.default_org.clone()))
    }

    fn stored_method(&self, org: Option<&str>) -> Option<String> {
        self.config
            .as_ref()
            .zip(org)
            .and_then(|(config, org)| config.orgs.get(org))
            .map(|entry| entry.auth.as_str().to_owned())
    }

    /// The flag-first environment: `--org`, `--pat` and `--server` in front of the
    /// environment they outrank.
    fn env(&self) -> FlagEnv<'_> {
        FlagEnv::new(self.base_env.as_ref())
            .set(ENV_ORG, self.opts.org.as_deref())
            .set(ENV_PAT, self.opts.pat.as_deref())
            .set(ENV_SERVER, self.opts.server.as_deref())
    }

    /// The credential this run uses: the flags and the environment first — neither
    /// reads the store — then the store for the organization, and when nothing
    /// answered at all, the one-time legacy import (W1-5).
    fn credentials(&mut self) -> Result<Credentials, AdoError> {
        match self.resolve() {
            Err(error) if error.code == ErrorCode::AuthRequired => {
                if self.import_legacy()? {
                    self.resolve()
                } else {
                    Err(error)
                }
            }
            resolved => resolved,
        }
    }

    fn resolve(&self) -> Result<Credentials, AdoError> {
        credentials::resolve(&self.env(), self.store.as_ref(), self.config.as_ref())
    }

    /// The one-time import of the Elixir CLI's config file (spec §7). The missing
    /// config file is what allows it and what makes it one-time: the credential is
    /// stored before the marker is written, so a failed import leaves no marker and
    /// the next run may import again, while a successful one is never repeated.
    fn import_legacy(&mut self) -> Result<bool, AdoError> {
        let mut config = self.config.clone().unwrap_or_default();
        let imported = legacy::import_once(
            &mut config,
            self.store.as_ref(),
            self.config_file.as_deref(),
            self.legacy_file.as_deref(),
        )?;

        if imported {
            self.config = Some(config);
        }

        Ok(imported)
    }
}

/// [`Option::filter`] with the blank-means-unset predicate, for a value that came
/// from a flag, the environment or a config file.
fn present(value: Option<String>) -> Option<String> {
    value.filter(|value| non_empty(value))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use ado_core::config::{CONFIG_FILE, OrgEntry};
    use ado_core::credentials::{InMemoryStore, SecretStore, StoreCall, Stored};
    use ado_core::env::MapEnv;
    use ado_core::error::ErrorCode;
    use ado_core::legacy::LEGACY_RELATIVE_PATH;
    use ado_testkit::{MockResponse, MockServer, TempHome};
    use serde_json::json;

    use super::*;

    /// What `basic_auth_headers(:env-pat)` sends: `Basic OmVudi1wYXQ=`.
    const BASIC_ENV_PAT: &str = "Basic OmVudi1wYXQ=";
    /// `Basic OmZsYWctcGF0` — `:flag-pat`.
    const BASIC_FLAG_PAT: &str = "Basic OmZsYWctcGF0";
    /// `Basic OmxlZ2FjeS1wYXQ=` — `:legacy-pat`.
    const BASIC_LEGACY_PAT: &str = "Basic OmxlZ2FjeS1wYXQ=";

    fn opts() -> GlobalOpts {
        GlobalOpts {
            org: None,
            pat: None,
            server: None,
            verbose: false,
            json: false,
        }
    }

    fn config_with(
        default_org: Option<&str>,
        server: Option<&str>,
        entry: Option<(&str, AuthMethod)>,
    ) -> Config {
        let mut config = Config {
            default_org: default_org.map(str::to_owned),
            server: server.map(str::to_owned),
            ..Config::default()
        };

        if let Some((org, auth)) = entry {
            config.orgs.insert(org.to_owned(), OrgEntry { auth });
        }

        config
    }

    fn stored(method: AuthMethod, token: &str) -> Stored {
        Stored {
            method,
            token: token.to_owned(),
        }
    }

    fn reads(store: &InMemoryStore) -> Vec<StoreCall> {
        store
            .calls()
            .into_iter()
            .filter(|call| matches!(call, StoreCall::Get(_)))
            .collect()
    }

    /// A context over a fixed environment, with no store and no files: no test
    /// reads the process environment or the developer's config directory.
    fn env_context(opts: GlobalOpts, env: &MapEnv, config: Option<Config>) -> Context {
        Context {
            opts,
            base_env: Box::new(env.clone()),
            config,
            store: Box::new(InMemoryStore::new()),
            config_file: None,
            legacy_file: None,
            client: None,
        }
    }

    /// The status tests' context: the flags and the config only.
    fn context(opts: GlobalOpts, config: Option<Config>) -> Context {
        env_context(opts, &MapEnv::new(), config)
    }

    /// A context over the environment, store and home a client test owns, so the
    /// test never reads the process environment, the OS keychain or the
    /// developer's files.
    fn client_context(
        opts: GlobalOpts,
        env: &MapEnv,
        store: &InMemoryStore,
        home: &TempHome,
    ) -> Context {
        Context {
            opts,
            base_env: Box::new(env.clone()),
            config: None,
            store: Box::new(store.clone()),
            config_file: Some(home.config_dir().join(CONFIG_FILE)),
            legacy_file: Some(home.path().join(LEGACY_RELATIVE_PATH)),
            client: None,
        }
    }

    #[test]
    fn json_or_report_forks_on_the_json_flag() {
        let json_context = context(
            GlobalOpts {
                json: true,
                ..opts()
            },
            None,
        );

        assert_eq!(
            json_context.json_or_report(json!({"ok": true}), || panic!(
                "the human report ran under --json"
            )),
            Report::Json(json!({"ok": true}))
        );

        let human_context = context(opts(), None);

        assert_eq!(
            human_context.json_or_report(json!({"ok": true}), || Report::Table {
                headers: vec!["ID".to_owned()],
                rows: vec![vec!["p1".to_owned()]],
            }),
            Report::Table {
                headers: vec!["ID".to_owned()],
                rows: vec![vec!["p1".to_owned()]],
            }
        );
    }

    #[test]
    fn the_environment_fills_globals_the_flags_left_unset() {
        let env = MapEnv::new()
            .set(ENV_ORG, "envorg")
            .set(ENV_PAT, "envpat")
            .set(ENV_SERVER, "https://env.example.com");

        let context = env_context(opts(), &env, None);
        let effective = context.env();

        assert_eq!(effective.get(ENV_ORG).as_deref(), Some("envorg"));
        assert_eq!(effective.get(ENV_PAT).as_deref(), Some("envpat"));
        assert_eq!(
            effective.get(ENV_SERVER).as_deref(),
            Some("https://env.example.com")
        );
    }

    #[test]
    fn flags_beat_the_environment() {
        let env = MapEnv::new().set(ENV_ORG, "envorg");
        let flagged = GlobalOpts {
            org: Some("flagorg".to_owned()),
            ..opts()
        };

        assert_eq!(
            env_context(flagged, &env, None)
                .env()
                .get(ENV_ORG)
                .as_deref(),
            Some("flagorg")
        );
    }

    #[test]
    fn blank_values_are_unset() {
        let env = MapEnv::new().set(ENV_ORG, "  ");
        let flagged = GlobalOpts {
            org: Some(String::new()),
            ..opts()
        };

        assert_eq!(env_context(opts(), &env, None).auth_status().org, None);
        assert_eq!(env_context(flagged, &env, None).auth_status().org, None);
    }

    #[test]
    fn flags_outrank_the_environment_at_the_client_boundary() {
        let home = TempHome::new();
        let server = MockServer::start();
        let env = MapEnv::new()
            .set(ENV_ORG, "envorg")
            .set(ENV_PAT, "env-pat")
            .set(ENV_SERVER, server.base_url());

        // No flags: the environment's organization and token reach the wire.
        let from_env_store = InMemoryStore::new();
        server.expect(
            "GET",
            "/envorg/_apis/projects",
            MockResponse::json(200, json!({"value": []})),
        );
        let mut from_env = client_context(opts(), &env, &from_env_store, &home);
        from_env
            .client()
            .expect("the environment resolves")
            .get("/_apis/projects", &[])
            .expect("the mock answers");

        // Flags for another organization: the flag organization and the flag PAT
        // are what the request carries. With `ADO_SERVER` pointing at the mock the
        // organization cannot show up in the host, so it shows up as the first path
        // segment — the same `Credentials::org` a cloud run puts in
        // `https://{org}.visualstudio.com`.
        let from_flags_store = InMemoryStore::new();
        server.expect(
            "GET",
            "/flagorg/_apis/projects",
            MockResponse::json(200, json!({"value": []})),
        );
        let flags = GlobalOpts {
            org: Some("flagorg".to_owned()),
            pat: Some("flag-pat".to_owned()),
            ..opts()
        };
        let mut from_flags = client_context(flags, &env, &from_flags_store, &home);
        from_flags
            .client()
            .expect("the flags resolve")
            .get("/_apis/projects", &[])
            .expect("the mock answers");

        // A flag server outranks `ADO_SERVER`: the request reaches the mock even
        // though the environment points at a port nothing listens on.
        let unreachable = MapEnv::new()
            .set(ENV_ORG, "envorg")
            .set(ENV_PAT, "env-pat")
            .set(ENV_SERVER, "http://127.0.0.1:1");
        server.expect(
            "GET",
            "/envorg/_apis/projects",
            MockResponse::json(200, json!({"value": []})),
        );
        let flag_server = GlobalOpts {
            server: Some(server.base_url().to_owned()),
            ..opts()
        };
        let mut from_server =
            client_context(flag_server, &unreachable, &InMemoryStore::new(), &home);
        from_server
            .client()
            .expect("the flag server resolves")
            .get("/_apis/projects", &[])
            .expect("the flag server wins");

        let received = server.received();
        assert_eq!(received.len(), 3);
        assert_eq!(received[0].path, "/envorg/_apis/projects");
        assert_eq!(
            received[0].header("authorization"),
            Some(BASIC_ENV_PAT),
            "ADO_PAT authenticates when no flag PAT is given"
        );
        assert_eq!(
            received[1].path, "/flagorg/_apis/projects",
            "the flag organization, not ADO_ORG, builds the request path"
        );
        assert_eq!(
            received[1].header("authorization"),
            Some(BASIC_FLAG_PAT),
            "the flag PAT authenticates, not ADO_PAT"
        );
        assert_eq!(
            received[2].path, "/envorg/_apis/projects",
            "the flag server, not ADO_SERVER, is the base"
        );
        assert!(
            reads(&from_env_store).is_empty() && reads(&from_flags_store).is_empty(),
            "a PAT from the environment or the flags is resolved without reading the store"
        );
    }

    #[test]
    fn a_second_client_request_reuses_the_resolved_credentials() {
        let home = TempHome::new();
        let server = MockServer::start();
        for _ in 0..2 {
            server.expect(
                "GET",
                "/myorg/_apis/projects",
                MockResponse::json(200, json!({"value": []})),
            );
        }
        let store = InMemoryStore::new();
        store
            .set("myorg", &stored(AuthMethod::Pat, "stored-pat"))
            .expect("seed the store");
        let env = MapEnv::new()
            .set(ENV_ORG, "myorg")
            .set(ENV_SERVER, server.base_url());
        let mut context = client_context(opts(), &env, &store, &home);

        for request in ["the first request", "the second request"] {
            context
                .client()
                .expect(request)
                .get("/_apis/projects", &[])
                .expect("the mock answers");
        }

        assert_eq!(server.received().len(), 2, "both requests went out");
        assert_eq!(
            reads(&store),
            vec![StoreCall::Get("myorg".to_owned())],
            "the credentials are resolved once per run, so the second request reuses them"
        );
    }

    #[test]
    fn the_legacy_file_is_imported_once_and_only_when_nothing_else_resolves() {
        let home = TempHome::new();
        let server = MockServer::start();
        server.expect(
            "GET",
            "/flagorg/_apis/projects",
            MockResponse::json(200, json!({"value": []})),
        );
        server.expect(
            "GET",
            "/legacyorg/_apis/projects",
            MockResponse::json(200, json!({"value": []})),
        );
        let legacy_file = home.path().join(LEGACY_RELATIVE_PATH);
        fs::create_dir_all(legacy_file.parent().expect("the legacy directory"))
            .expect("create the legacy directory");
        fs::write(
            &legacy_file,
            r#"{"org":"legacyorg","method":"pat","pat":"legacy-pat"}"#,
        )
        .expect("write the legacy file");
        let env = MapEnv::new().set(ENV_SERVER, server.base_url());
        let config_file = home.config_dir().join(CONFIG_FILE);

        // Something else resolves: no import, so no config file appears and the
        // store is not touched.
        let flag_store = InMemoryStore::new();
        let flags = GlobalOpts {
            org: Some("flagorg".to_owned()),
            pat: Some("flag-pat".to_owned()),
            ..opts()
        };
        let mut flagged = client_context(flags, &env, &flag_store, &home);
        flagged
            .client()
            .expect("the flag resolves")
            .get("/_apis/projects", &[])
            .expect("the mock answers");

        assert!(
            !config_file.exists(),
            "a credential that resolves must not import the legacy file"
        );
        assert!(
            flag_store.calls().is_empty(),
            "the environment needs no store call either"
        );

        // Nothing resolves: the legacy file imports, and its organization and
        // token are what the request carries.
        let legacy_store = InMemoryStore::new();
        let mut empty = client_context(opts(), &env, &legacy_store, &home);
        empty
            .client()
            .expect("the legacy file resolves")
            .get("/_apis/projects", &[])
            .expect("the mock answers");

        let received = server.received();
        assert_eq!(received[1].path, "/legacyorg/_apis/projects");
        assert_eq!(received[1].header("authorization"), Some(BASIC_LEGACY_PAT));
        assert!(
            config_file.exists(),
            "the import leaves the config file behind as its marker"
        );
        assert_eq!(
            legacy_store
                .get("legacyorg")
                .expect("the imported credential"),
            Some(stored(AuthMethod::Pat, "legacy-pat"))
        );

        // A later run finds the marker: nothing imports a second time, so an empty
        // store stays empty and the run reports auth_required.
        let fresh_store = InMemoryStore::new();
        let mut later = client_context(opts(), &env, &fresh_store, &home);
        let error = later
            .client()
            .err()
            .expect("the marker stops the import, so nothing resolves");

        assert_eq!(error.code, ErrorCode::AuthRequired);
        assert!(
            fresh_store.calls().is_empty(),
            "the legacy credential is stored once, never again"
        );
    }

    #[test]
    fn config_fills_org_server_and_method() {
        let config = config_with(
            Some("myorg"),
            Some("https://ado.example.com"),
            Some(("myorg", AuthMethod::Device)),
        );

        let status = context(opts(), Some(config)).auth_status();

        assert!(status.configured);
        assert!(status.authenticated);
        assert_eq!(status.org.as_deref(), Some("myorg"));
        assert_eq!(status.server, "https://ado.example.com");
        assert_eq!(status.method.as_deref(), Some("device"));
    }

    #[test]
    fn flag_org_beats_the_config_default_org() {
        let config = config_with(
            Some("configorg"),
            None,
            Some(("configorg", AuthMethod::Device)),
        );
        let flagged = GlobalOpts {
            org: Some("flagorg".to_owned()),
            ..opts()
        };

        let status = context(flagged, Some(config)).auth_status();

        assert_eq!(status.org.as_deref(), Some("flagorg"));
        assert_eq!(
            status.method, None,
            "the entry belongs to the config's org, not the resolved one"
        );
    }

    #[test]
    fn runtime_pat_reports_the_pat_method_without_config() {
        let pat = GlobalOpts {
            pat: Some("pat-token".to_owned()),
            ..opts()
        };

        let status = context(pat, None).auth_status();

        assert!(!status.configured);
        assert!(
            !status.authenticated,
            "a token without an organization is not an authenticated setup"
        );
        assert_eq!(status.org, None);
        assert_eq!(status.method.as_deref(), Some("pat"));
        assert_eq!(status.server, DEFAULT_SERVER);
    }

    #[test]
    fn without_config_or_org_nothing_is_authenticated() {
        let status = context(opts(), None).auth_status();

        assert!(!status.configured);
        assert!(!status.authenticated);
        assert_eq!(status.org, None);
        assert_eq!(status.method, None);
        assert_eq!(status.server, DEFAULT_SERVER);
    }

    #[test]
    fn the_config_entry_beats_the_runtime_pat() {
        let config = config_with(Some("myorg"), None, Some(("myorg", AuthMethod::Browser)));
        let pat = GlobalOpts {
            pat: Some("pat-token".to_owned()),
            ..opts()
        };

        let status = context(pat, Some(config)).auth_status();

        assert_eq!(status.method.as_deref(), Some("browser"));
    }
}
