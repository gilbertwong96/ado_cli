//! The config file, `<OS config dir>/ado/config.toml`. It never holds a secret:
//! tokens live in the OS keychain or in `credentials.json` (spec §7).

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::AdoError;

/// The config file name inside the config directory.
pub const CONFIG_FILE: &str = "config.toml";

/// How the token for an organization was obtained.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuthMethod {
    Pat,
    Device,
    Browser,
}

impl AuthMethod {
    /// The wire spelling serde already emits: `"pat"`, `"device"`, `"browser"`.
    pub fn as_str(&self) -> &'static str {
        match self {
            AuthMethod::Pat => "pat",
            AuthMethod::Device => "device",
            AuthMethod::Browser => "browser",
        }
    }
}

/// The non-secret settings for one organization.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrgEntry {
    pub auth: AuthMethod,
}

/// `config.toml` in memory.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_org: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub orgs: BTreeMap<String, OrgEntry>,
}

/// `<OS config dir>/ado` — where `config.toml` and `credentials.json` live.
/// `None` when the OS cannot resolve a config directory (`HOME` unset).
pub fn config_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join("ado"))
}

/// `<config dir>/ado/config.toml`.
pub fn config_path() -> Option<PathBuf> {
    config_dir().map(|dir| dir.join(CONFIG_FILE))
}

/// Reads the config. A missing, unreadable or malformed file reads as absent:
/// a hand-broken config must never stop the CLI from running.
pub fn load() -> Result<Option<Config>, AdoError> {
    match config_path() {
        Some(path) => load_at(&path),
        None => Ok(None),
    }
}

/// Writes the config, creating the config directory when needed.
pub fn save(config: &Config) -> Result<(), AdoError> {
    save_at(&config_path().ok_or_else(missing_config_dir)?, config)
}

pub(crate) fn load_at(path: &Path) -> Result<Option<Config>, AdoError> {
    let Ok(text) = fs::read_to_string(path) else {
        return Ok(None);
    };

    Ok(toml::from_str(&text).ok())
}

/// Writes the config to `path`. The caller owns the path it resolved — the binary's
/// `Context` writes the config file the run already loaded, so a test writes its own
/// temp directory and never the developer's — where [`save`] resolves the OS
/// config directory itself.
pub fn save_at(path: &Path, config: &Config) -> Result<(), AdoError> {
    let text = toml::to_string_pretty(config)
        .map_err(|error| AdoError::validation(format!("Cannot encode config: {error}")))?;

    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|error| write_failed(path, &error))?;
    }

    fs::write(path, text).map_err(|error| write_failed(path, &error))
}

/// Removes the config file at `path`. An absent file is not an error: a logout is
/// idempotent, and the second run finds nothing to delete.
pub fn delete_at(path: &Path) -> Result<(), AdoError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(write_failed(path, &error)),
    }
}

pub(crate) fn missing_config_dir() -> AdoError {
    AdoError::validation("Cannot determine the config directory: HOME is not set.")
}

pub(crate) fn write_failed(path: &Path, error: &std::io::Error) -> AdoError {
    AdoError::validation(format!("Cannot write {}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use std::fs;

    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    use super::*;
    use crate::error::ErrorCode;
    use crate::test_support::TempDir;

    fn sample() -> Config {
        Config {
            default_org: Some("myorg".to_owned()),
            server: Some("https://dev.azure.com".to_owned()),
            orgs: BTreeMap::from([(
                "myorg".to_owned(),
                OrgEntry {
                    auth: AuthMethod::Pat,
                },
            )]),
        }
    }

    #[test]
    fn auth_method_as_str_matches_the_serde_wire_form() {
        for method in [AuthMethod::Pat, AuthMethod::Device, AuthMethod::Browser] {
            assert_eq!(
                serde_json::to_value(method).expect("serialise"),
                serde_json::json!(method.as_str()),
                "{method:?}"
            );
        }
    }

    #[test]
    fn config_round_trips_through_toml() {
        let dir = TempDir::new("config-round-trip");
        let path = dir.path().join("ado").join("config.toml");

        save_at(&path, &sample()).expect("save");

        let text = fs::read_to_string(&path).expect("config.toml written");
        assert_eq!(
            text,
            "default_org = \"myorg\"\nserver = \"https://dev.azure.com\"\n\n[orgs.myorg]\nauth = \"pat\"\n"
        );
        assert_eq!(load_at(&path).expect("load"), Some(sample()));
    }

    #[test]
    fn config_load_ignores_malformed_toml() {
        let dir = TempDir::new("config-malformed");

        let cases = [
            ("truncated", "default_org = \"myorg\"\n[orgs.myorg"),
            ("garbage", "\u{0}\u{1} not toml at all"),
            ("wrong-type", "default_org = 7"),
            ("unknown-method", "[orgs.myorg]\nauth = \"az_cli\"\n"),
        ];

        for (label, text) in cases {
            let path = dir.path().join(format!("{label}.toml"));
            fs::write(&path, text).expect("write");

            assert_eq!(
                load_at(&path).expect("a malformed config is not an error"),
                None,
                "{label}"
            );
        }

        assert_eq!(
            load_at(&dir.path().join("absent.toml")).expect("a missing config is not an error"),
            None
        );
    }

    #[test]
    fn config_path_uses_the_os_config_dir() {
        assert_eq!(
            config_path(),
            dirs::config_dir().map(|dir| dir.join("ado").join("config.toml"))
        );
        assert_eq!(
            config_dir(),
            config_path().and_then(|path| path.parent().map(Path::to_path_buf))
        );
    }

    #[test]
    fn delete_at_removes_the_file_and_tolerates_absence() {
        let dir = TempDir::new("config-delete");
        let path = dir.path().join("ado").join("config.toml");
        save_at(&path, &sample()).expect("save");

        delete_at(&path).expect("delete");

        assert!(!path.exists(), "the file is removed");
        delete_at(&path).expect("an absent file is not an error");
    }

    #[cfg(unix)]
    #[test]
    fn save_fails_cleanly_on_readonly_dir() {
        let dir = TempDir::new("config-readonly");
        let ado_dir = dir.path().join("ado");
        fs::create_dir_all(&ado_dir).expect("mkdir");
        fs::set_permissions(&ado_dir, fs::Permissions::from_mode(0o555)).expect("chmod");

        if fs::write(ado_dir.join("probe"), b"probe").is_ok() {
            // Running as root: the mode bits do not apply, so there is nothing to assert.
            fs::remove_file(ado_dir.join("probe")).expect("remove probe");
            fs::set_permissions(&ado_dir, fs::Permissions::from_mode(0o755)).expect("chmod back");

            return;
        }

        let saved = save_at(&ado_dir.join("config.toml"), &sample());
        fs::set_permissions(&ado_dir, fs::Permissions::from_mode(0o755)).expect("chmod back");

        let error = saved.expect_err("a read-only config dir must fail cleanly");

        assert_eq!(error.code, ErrorCode::ValidationError);
        assert!(
            error.message.contains("config.toml"),
            "message: {}",
            error.message
        );
    }
}
