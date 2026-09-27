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

/// The flags in front of an environment: a value set here outranks the same
/// variable in `base`, so `--org` and `--pat` beat `ADO_ORG` and `ADO_PAT`
/// (spec §6.6). A blank flag value means "not provided", like a blank variable.
pub struct FlagEnv<'a> {
    flags: BTreeMap<&'static str, String>,
    base: &'a dyn EnvSource,
}

impl<'a> FlagEnv<'a> {
    pub fn new(base: &'a dyn EnvSource) -> FlagEnv<'a> {
        FlagEnv {
            flags: BTreeMap::new(),
            base,
        }
    }

    /// The flag value for the variable it outranks.
    pub fn set(mut self, key: &'static str, value: Option<&str>) -> FlagEnv<'a> {
        if let Some(value) = value.filter(|value| !value.trim().is_empty()) {
            self.flags.insert(key, value.to_owned());
        }
        self
    }
}

impl EnvSource for FlagEnv<'_> {
    fn get(&self, key: &str) -> Option<String> {
        self.flags.get(key).cloned().or_else(|| self.base.get(key))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_flag_outranks_the_base_environment() {
        let base = MapEnv::new()
            .set(ENV_ORG, "envorg")
            .set(ENV_SERVER, "https://env.example.com");
        let flags = FlagEnv::new(&base).set(ENV_ORG, Some("flagorg"));

        assert_eq!(flags.get(ENV_ORG).as_deref(), Some("flagorg"));
        assert_eq!(
            flags.get(ENV_SERVER).as_deref(),
            Some("https://env.example.com")
        );
        assert_eq!(
            FlagEnv::new(&base)
                .set(ENV_ORG, None)
                .get(ENV_ORG)
                .as_deref(),
            Some("envorg")
        );
    }

    #[test]
    fn a_blank_flag_is_unset() {
        let base = MapEnv::new().set(ENV_ORG, "envorg");

        assert_eq!(
            FlagEnv::new(&base)
                .set(ENV_ORG, Some("  "))
                .get(ENV_ORG)
                .as_deref(),
            Some("envorg")
        );
    }
}
