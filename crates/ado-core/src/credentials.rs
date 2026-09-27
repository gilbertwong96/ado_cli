//! Where the token lives: the OS keychain first, then
//! `<config dir>/ado/credentials.json` — `{"<org>": {"method": "pat", "token": "…"}}` —
//! when the keychain is unusable, and never `config.toml` (spec §7).

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::config::{AuthMethod, Config, config_dir, missing_config_dir, write_failed};
use crate::env::{ENV_ORG, ENV_PAT, EnvSource};
use crate::error::AdoError;

/// The keychain service name; the account is the organization.
pub const KEYCHAIN_SERVICE: &str = "ado";
/// The fallback credential file, inside the config directory.
pub const CREDENTIALS_FILE: &str = "credentials.json";

/// One stored credential: how the token was obtained, and the token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stored {
    pub method: AuthMethod,
    pub token: String,
}

/// A credential ready to use against one organization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Credentials {
    pub org: String,
    pub method: AuthMethod,
    pub token: String,
}

/// A place a token can live, keyed by organization.
pub trait SecretStore {
    fn get(&self, org: &str) -> Result<Option<Stored>, AdoError>;
    fn set(&self, org: &str, stored: &Stored) -> Result<(), AdoError>;
    fn delete(&self, org: &str) -> Result<(), AdoError>;
}

/// The OS keychain: macOS Keychain, Windows Credential Manager, Linux secret
/// service, keyed by service `ado` and the organization.
pub struct KeychainStore {
    service: String,
}

impl KeychainStore {
    pub fn new() -> KeychainStore {
        KeychainStore {
            service: KEYCHAIN_SERVICE.to_owned(),
        }
    }

    fn entry(&self, org: &str) -> Result<keyring::Entry, AdoError> {
        keyring::Entry::new(&self.service, org).map_err(keychain_unavailable)
    }
}

impl Default for KeychainStore {
    fn default() -> KeychainStore {
        KeychainStore::new()
    }
}

impl SecretStore for KeychainStore {
    fn get(&self, org: &str) -> Result<Option<Stored>, AdoError> {
        match self.entry(org)?.get_password() {
            Ok(password) => Ok(serde_json::from_str(&password).ok()),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(keychain_unavailable(error)),
        }
    }

    fn set(&self, org: &str, stored: &Stored) -> Result<(), AdoError> {
        let password = serde_json::to_string(stored)
            .map_err(|error| AdoError::validation(format!("Cannot encode credential: {error}")))?;

        self.entry(org)?
            .set_password(&password)
            .map_err(keychain_unavailable)
    }

    fn delete(&self, org: &str) -> Result<(), AdoError> {
        match self.entry(org)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(keychain_unavailable(error)),
        }
    }
}

fn keychain_unavailable(error: keyring::Error) -> AdoError {
    AdoError::validation(format!("OS keychain unavailable: {error}"))
}

/// `credentials.json` in the config directory, written mode 0600 on Unix. A
/// malformed file reads as absent, like a malformed config.
#[derive(Debug, Clone)]
pub struct FileStore {
    path: Option<PathBuf>,
}

impl FileStore {
    pub fn new(path: PathBuf) -> FileStore {
        FileStore { path: Some(path) }
    }

    /// `<config dir>/ado/credentials.json`; a store that can only report absence
    /// when the OS cannot resolve a config directory.
    pub fn from_config_dir() -> FileStore {
        FileStore {
            path: config_dir().map(|dir| dir.join(CREDENTIALS_FILE)),
        }
    }

    fn entries(&self) -> BTreeMap<String, Stored> {
        let Some(path) = self.path.as_deref() else {
            return BTreeMap::new();
        };
        let Ok(text) = fs::read_to_string(path) else {
            return BTreeMap::new();
        };

        serde_json::from_str(&text).unwrap_or_default()
    }

    fn write(&self, entries: &BTreeMap<String, Stored>) -> Result<(), AdoError> {
        let path = self.path.as_deref().ok_or_else(missing_config_dir)?;
        let bytes = serde_json::to_vec_pretty(entries)
            .map_err(|error| AdoError::validation(format!("Cannot encode credentials: {error}")))?;

        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(|error| write_failed(path, &error))?;
        }

        write_private(path, &bytes)
    }
}

impl SecretStore for FileStore {
    fn get(&self, org: &str) -> Result<Option<Stored>, AdoError> {
        Ok(self.entries().get(org).cloned())
    }

    fn set(&self, org: &str, stored: &Stored) -> Result<(), AdoError> {
        let mut entries = self.entries();
        entries.insert(org.to_owned(), stored.clone());

        self.write(&entries)
    }

    fn delete(&self, org: &str) -> Result<(), AdoError> {
        let Some(path) = self.path.as_deref() else {
            return Ok(());
        };

        let mut entries = self.entries();
        if entries.remove(org).is_none() {
            return Ok(());
        }

        if entries.is_empty() {
            return match fs::remove_file(path) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(write_failed(path, &error)),
            };
        }

        self.write(&entries)
    }
}

#[cfg(unix)]
fn write_private(path: &Path, bytes: &[u8]) -> Result<(), AdoError> {
    use std::io::Write;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        .map_err(|error| write_failed(path, &error))?;
    file.write_all(bytes)
        .map_err(|error| write_failed(path, &error))?;

    // `mode` only applies to a newly created file, so an existing one is tightened here.
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|error| write_failed(path, &error))
}

#[cfg(not(unix))]
fn write_private(path: &Path, bytes: &[u8]) -> Result<(), AdoError> {
    fs::write(path, bytes).map_err(|error| write_failed(path, &error))
}

fn unavailable_store() -> AdoError {
    AdoError::validation("Credential store unavailable.")
}

/// Two stores in order: the keychain, then the credentials file, so a locked
/// keyring or a headless machine still finds an imported token.
pub struct FallbackStore {
    primary: Box<dyn SecretStore>,
    fallback: Box<dyn SecretStore>,
}

impl FallbackStore {
    pub fn new(primary: impl SecretStore + 'static, fallback: impl SecretStore + 'static) -> Self {
        FallbackStore {
            primary: Box::new(primary),
            fallback: Box::new(fallback),
        }
    }
}

impl SecretStore for FallbackStore {
    fn get(&self, org: &str) -> Result<Option<Stored>, AdoError> {
        match self.primary.get(org) {
            Ok(Some(stored)) => Ok(Some(stored)),
            Ok(None) | Err(_) => self.fallback.get(org),
        }
    }

    fn set(&self, org: &str, stored: &Stored) -> Result<(), AdoError> {
        match self.primary.set(org, stored) {
            Ok(()) => Ok(()),
            Err(_) => self.fallback.set(org, stored),
        }
    }

    fn delete(&self, org: &str) -> Result<(), AdoError> {
        self.fallback.delete(org)?;
        self.primary.delete(org)
    }
}

/// The store the binary uses: the keychain, with the credentials file behind it.
pub fn default_store() -> FallbackStore {
    FallbackStore::new(KeychainStore::new(), FileStore::from_config_dir())
}

/// Resolves the credential to use: `ADO_PAT` (with `ADO_ORG`) first, then the
/// store, which is the keychain with the credentials file behind it. `ADO_SERVER`
/// travels with those but is read by the caller.
pub fn resolve(
    env: &dyn EnvSource,
    store: &dyn SecretStore,
    config: Option<&Config>,
) -> Result<Credentials, AdoError> {
    let Some(org) = organization(env, config) else {
        return Err(AdoError::auth_required());
    };

    if let Some(token) = env.get(ENV_PAT).filter(|pat| !pat.trim().is_empty()) {
        return Ok(Credentials {
            org,
            method: AuthMethod::Pat,
            token,
        });
    }

    match store.get(&org)? {
        Some(stored) => Ok(Credentials {
            org,
            method: stored.method,
            token: stored.token,
        }),
        None => Err(AdoError::auth_required()),
    }
}

fn organization(env: &dyn EnvSource, config: Option<&Config>) -> Option<String> {
    let from_env = env.get(ENV_ORG).filter(|org| !org.trim().is_empty());
    let from_config = config
        .and_then(|config| config.default_org.clone())
        .filter(|org| !org.trim().is_empty());

    from_env.or(from_config)
}

/// One call an [`InMemoryStore`] received.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreCall {
    Get(String),
    Set(String),
    Delete(String),
}

/// A test double that keeps every credential in memory and records the calls it
/// received, so a test can prove the OS keychain was never on the path.
#[derive(Debug, Clone, Default)]
pub struct InMemoryStore {
    state: Arc<InMemoryState>,
}

#[derive(Debug, Default)]
struct InMemoryState {
    entries: Mutex<BTreeMap<String, Stored>>,
    calls: Mutex<Vec<StoreCall>>,
    unavailable: bool,
}

impl InMemoryStore {
    pub fn new() -> InMemoryStore {
        InMemoryStore::default()
    }

    /// A store whose backend refuses every call, standing in for a keychain that
    /// is locked or missing.
    pub fn unavailable() -> InMemoryStore {
        InMemoryStore {
            state: Arc::new(InMemoryState {
                unavailable: true,
                ..InMemoryState::default()
            }),
        }
    }

    pub fn calls(&self) -> Vec<StoreCall> {
        lock(&self.state.calls).clone()
    }

    fn record(&self, call: StoreCall) {
        lock(&self.state.calls).push(call);
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|error| error.into_inner())
}

impl SecretStore for InMemoryStore {
    fn get(&self, org: &str) -> Result<Option<Stored>, AdoError> {
        self.record(StoreCall::Get(org.to_owned()));

        if self.state.unavailable {
            return Err(unavailable_store());
        }

        Ok(lock(&self.state.entries).get(org).cloned())
    }

    fn set(&self, org: &str, stored: &Stored) -> Result<(), AdoError> {
        self.record(StoreCall::Set(org.to_owned()));

        if self.state.unavailable {
            return Err(unavailable_store());
        }

        lock(&self.state.entries).insert(org.to_owned(), stored.clone());

        Ok(())
    }

    fn delete(&self, org: &str) -> Result<(), AdoError> {
        self.record(StoreCall::Delete(org.to_owned()));

        if self.state.unavailable {
            return Err(unavailable_store());
        }

        lock(&self.state.entries).remove(org);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    use super::*;
    use crate::env::MapEnv;
    use crate::error::ErrorCode;
    use crate::test_support::TempDir;

    fn stored(method: AuthMethod, token: &str) -> Stored {
        Stored {
            method,
            token: token.to_owned(),
        }
    }

    fn credentials(org: &str, method: AuthMethod, token: &str) -> Credentials {
        Credentials {
            org: org.to_owned(),
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

    fn config_with_org(org: &str) -> Config {
        Config {
            default_org: Some(org.to_owned()),
            ..Config::default()
        }
    }

    #[cfg(unix)]
    #[test]
    fn credentials_file_is_0600() {
        let dir = TempDir::new("credentials-mode");
        let path = dir.path().join("ado").join(CREDENTIALS_FILE);
        let store = FileStore::new(path.clone());

        store
            .set("myorg", &stored(AuthMethod::Pat, "pat-token"))
            .expect("set");

        let mode = fs::metadata(&path)
            .expect("credentials file written")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600, "credentials file mode: {mode:o}");

        let written: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).expect("read")).expect("json");
        assert_eq!(
            written,
            serde_json::json!({"myorg": {"method": "pat", "token": "pat-token"}})
        );
        assert_eq!(
            store.get("myorg").expect("get"),
            Some(stored(AuthMethod::Pat, "pat-token"))
        );
    }

    #[test]
    fn credentials_file_malformed_is_absent() {
        let dir = TempDir::new("credentials-malformed");
        let path = dir.path().join(CREDENTIALS_FILE);
        fs::write(&path, r#"{"myorg":{"method":"pat","tok"#).expect("write");
        let store = FileStore::new(path);

        assert_eq!(store.get("myorg").expect("malformed is not an error"), None);
    }

    #[test]
    fn delete_removes_the_credential_from_every_layer() {
        let dir = TempDir::new("delete");
        let path = dir.path().join(CREDENTIALS_FILE);
        let file = FileStore::new(path.clone());
        let keychain = InMemoryStore::new();
        let store = FallbackStore::new(keychain.clone(), file.clone());
        file.set("myorg", &stored(AuthMethod::Pat, "file-token"))
            .expect("seed file");
        keychain
            .set("myorg", &stored(AuthMethod::Pat, "keychain-token"))
            .expect("seed keychain");

        store.delete("myorg").expect("delete");

        assert_eq!(store.get("myorg").expect("get"), None);
        assert!(!path.exists(), "an empty credentials file is removed");
        file.delete("myorg").expect("delete is idempotent");
    }

    #[test]
    fn resolution_prefers_env_over_store_and_file() {
        let dir = TempDir::new("resolution-env");
        let file = FileStore::new(dir.path().join(CREDENTIALS_FILE));
        file.set("myorg", &stored(AuthMethod::Device, "file-token"))
            .expect("seed file");
        let keychain = InMemoryStore::new();
        keychain
            .set("myorg", &stored(AuthMethod::Browser, "keychain-token"))
            .expect("seed keychain");
        let store = FallbackStore::new(keychain.clone(), file);
        let env = MapEnv::new()
            .set(ENV_ORG, "myorg")
            .set(ENV_PAT, "env-token");

        let resolved = resolve(&env, &store, None).expect("env credentials");

        assert_eq!(resolved, credentials("myorg", AuthMethod::Pat, "env-token"));
        assert!(
            reads(&keychain).is_empty(),
            "ADO_PAT wins without reading the store"
        );
    }

    #[test]
    fn resolution_falls_back_to_file_when_keychain_unavailable() {
        let dir = TempDir::new("resolution-file");
        let file = FileStore::new(dir.path().join(CREDENTIALS_FILE));
        file.set("myorg", &stored(AuthMethod::Pat, "file-token"))
            .expect("seed file");
        let keychain = InMemoryStore::unavailable();
        let store = FallbackStore::new(keychain.clone(), file);
        let config = config_with_org("myorg");

        let resolved = resolve(&MapEnv::new(), &store, Some(&config)).expect("file credentials");

        assert_eq!(
            resolved,
            credentials("myorg", AuthMethod::Pat, "file-token")
        );
        assert_eq!(
            reads(&keychain),
            vec![StoreCall::Get("myorg".to_owned())],
            "the unavailable keychain is tried first"
        );
    }

    #[test]
    fn keychain_store_is_not_used_in_tests() {
        let dir = TempDir::new("resolution-in-memory");
        let keychain = InMemoryStore::new();
        keychain
            .set("myorg", &stored(AuthMethod::Pat, "in-memory-token"))
            .expect("seed");
        let store = FallbackStore::new(
            keychain.clone(),
            FileStore::new(dir.path().join(CREDENTIALS_FILE)),
        );
        let config = config_with_org("myorg");

        let resolved =
            resolve(&MapEnv::new(), &store, Some(&config)).expect("in-memory credentials");

        assert_eq!(
            resolved,
            credentials("myorg", AuthMethod::Pat, "in-memory-token")
        );
        assert_eq!(
            reads(&keychain),
            vec![StoreCall::Get("myorg".to_owned())],
            "the injected double is the only source consulted, so no OS keychain is reached"
        );
    }

    #[test]
    fn resolution_without_any_credential_is_auth_required() {
        let store = InMemoryStore::new();

        let no_credential = resolve(&MapEnv::new(), &store, Some(&config_with_org("myorg")))
            .expect_err("no credential anywhere");
        assert_eq!(no_credential.code, ErrorCode::AuthRequired);

        let no_org = resolve(&MapEnv::new(), &store, None).expect_err("no org either");
        assert_eq!(no_org.code, ErrorCode::AuthRequired);
    }

    #[test]
    fn fallback_store_writes_to_the_keychain_when_it_works() {
        let dir = TempDir::new("fallback-write");
        let file = FileStore::new(dir.path().join(CREDENTIALS_FILE));
        let keychain = InMemoryStore::new();
        let store = FallbackStore::new(keychain.clone(), file.clone());

        store
            .set("myorg", &stored(AuthMethod::Pat, "pat-token"))
            .expect("set");

        assert_eq!(
            keychain.get("myorg").expect("get"),
            Some(stored(AuthMethod::Pat, "pat-token"))
        );
        assert_eq!(
            file.get("myorg").expect("get"),
            None,
            "the file is only a fallback"
        );
    }

    #[test]
    fn fallback_store_writes_to_the_file_when_the_keychain_is_unavailable() {
        let dir = TempDir::new("fallback-file");
        let file = FileStore::new(dir.path().join(CREDENTIALS_FILE));
        let store = FallbackStore::new(InMemoryStore::unavailable(), file.clone());

        store
            .set("myorg", &stored(AuthMethod::Pat, "pat-token"))
            .expect("set");

        assert_eq!(
            file.get("myorg").expect("get"),
            Some(stored(AuthMethod::Pat, "pat-token"))
        );
    }
}
