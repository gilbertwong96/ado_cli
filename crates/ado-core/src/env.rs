//! The environment `ado` reads. Commands take an [`EnvSource`] so `ADO_*` behaviour
//! can be driven by tests without mutating the process environment.

use std::collections::BTreeMap;

/// The organization: `ADO_ORG`.
pub const ENV_ORG: &str = "ADO_ORG";
/// The personal access token: `ADO_PAT`.
pub const ENV_PAT: &str = "ADO_PAT";
/// The base URL of a self-hosted Azure DevOps Server: `ADO_SERVER`.
pub const ENV_SERVER: &str = "ADO_SERVER";

pub trait EnvSource {
    fn get(&self, key: &str) -> Option<String>;
}

/// The real process environment.
pub struct ProcessEnv;

impl EnvSource for ProcessEnv {
    fn get(&self, key: &str) -> Option<String> {
        std::env::var(key).ok()
    }
}

/// A fixed environment, for tests.
#[derive(Debug, Default, Clone)]
pub struct MapEnv {
    vars: BTreeMap<String, String>,
}

impl MapEnv {
    pub fn new() -> MapEnv {
        MapEnv::default()
    }

    pub fn set(mut self, key: &str, value: &str) -> MapEnv {
        self.vars.insert(key.to_owned(), value.to_owned());
        self
    }
}

impl EnvSource for MapEnv {
    fn get(&self, key: &str) -> Option<String> {
        self.vars.get(key).cloned()
    }
}
