//! The per-invocation context: the effective global options and the config file.
//!
//! Loading is deliberately read-only (ruling R37): it never opens the credential
//! store and never imports the legacy config, so a status command cannot trigger
//! a keychain prompt. The auth method lives in `config.toml`; the token does not.

use std::path::PathBuf;

use ado_core::config::{self, AuthMethod, Config};
use ado_core::env::{ENV_ORG, ENV_PAT, ENV_SERVER, EnvSource, ProcessEnv};

use crate::args::GlobalOpts;

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

/// The effective global options and the loaded config file.
pub struct Context {
    opts: GlobalOpts,
    config: Option<Config>,
}

impl Context {
    /// Reads the config file and overlays the `ADO_*` environment onto the
    /// globals the command line left unset.
    pub fn load(opts: GlobalOpts) -> Context {
        Context {
            opts: overlay(opts, &ProcessEnv),
            config: config::load().unwrap_or_default(),
        }
    }

    /// Whether the invocation asked for JSON output.
    pub fn json(&self) -> bool {
        self.opts.json
    }

    /// The status `whoami` reports: a config file that loads sets `configured`,
    /// an organization in the flags, environment or config sets `org`, and the
    /// method is the org's `config.toml` entry, else `pat` when a runtime PAT is
    /// present — never the token itself. `config_file` is `None` only when the OS
    /// cannot resolve a config directory: on macOS `dirs` falls back to the passwd
    /// database when `HOME` is unset, so a normal user always resolves a path.
    pub fn auth_status(&self) -> AuthStatus {
        let configured = self.config.is_some();
        let org = non_empty(self.opts.org.clone()).or_else(|| self.default_org());
        let server = non_empty(self.opts.server.clone())
            .or_else(|| {
                self.config
                    .as_ref()
                    .and_then(|config| non_empty(config.server.clone()))
            })
            .unwrap_or_else(|| DEFAULT_SERVER.to_owned());
        let method = self.stored_method(org.as_deref()).or_else(|| {
            non_empty(self.opts.pat.clone()).map(|_| AuthMethod::Pat.as_str().to_owned())
        });

        AuthStatus {
            configured,
            authenticated: configured || org.is_some(),
            org,
            server,
            method,
            config_file: config::config_path(),
        }
    }

    fn default_org(&self) -> Option<String> {
        self.config
            .as_ref()
            .and_then(|config| non_empty(config.default_org.clone()))
    }

    fn stored_method(&self, org: Option<&str>) -> Option<String> {
        self.config
            .as_ref()
            .zip(org)
            .and_then(|(config, org)| config.orgs.get(org))
            .map(|entry| entry.auth.as_str().to_owned())
    }
}

/// `--flag` first, then `ADO_*`, with blank values read as unset.
fn overlay(opts: GlobalOpts, env: &dyn EnvSource) -> GlobalOpts {
    GlobalOpts {
        org: non_empty(opts.org).or_else(|| non_empty(env.get(ENV_ORG))),
        pat: non_empty(opts.pat).or_else(|| non_empty(env.get(ENV_PAT))),
        server: non_empty(opts.server).or_else(|| non_empty(env.get(ENV_SERVER))),
        verbose: opts.verbose,
        json: opts.json,
    }
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ado_core::config::OrgEntry;
    use ado_core::env::MapEnv;

    fn opts() -> GlobalOpts {
        GlobalOpts {
            org: None,
            pat: None,
            server: None,
            verbose: false,
            json: false,
        }
    }

    fn context(opts: GlobalOpts, config: Option<Config>) -> Context {
        Context { opts, config }
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

    #[test]
    fn env_fills_globals_the_flags_left_unset() {
        let env = MapEnv::new()
            .set(ENV_ORG, "envorg")
            .set(ENV_PAT, "envpat")
            .set(ENV_SERVER, "https://env.example.com");

        let overlaid = overlay(opts(), &env);

        assert_eq!(overlaid.org.as_deref(), Some("envorg"));
        assert_eq!(overlaid.pat.as_deref(), Some("envpat"));
        assert_eq!(overlaid.server.as_deref(), Some("https://env.example.com"));
    }

    #[test]
    fn flags_beat_the_environment() {
        let env = MapEnv::new().set(ENV_ORG, "envorg");
        let flagged = GlobalOpts {
            org: Some("flagorg".to_owned()),
            ..opts()
        };

        assert_eq!(overlay(flagged, &env).org.as_deref(), Some("flagorg"));
    }

    #[test]
    fn blank_values_are_unset() {
        let env = MapEnv::new().set(ENV_ORG, "  ");
        let flagged = GlobalOpts {
            org: Some(String::new()),
            ..opts()
        };

        assert_eq!(overlay(opts(), &env).org, None);
        assert_eq!(overlay(flagged, &env).org, None);
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
