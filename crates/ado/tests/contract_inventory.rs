//! The contract inventory checked against the argv, environment and REST
//! surface it records.

use std::fs;
use std::path::PathBuf;

use ado::argv::MULTIVALUE_FLAGS;
use ado_core::client::API_VERSION;
use ado_core::env::{ENV_ORG, ENV_PAT, ENV_SERVER};

fn inventory_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/rust-rewrite/contract-inventory.md")
}

fn inventory() -> String {
    let path = inventory_path();

    fs::read_to_string(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

#[test]
fn inventory_records_the_multivalue_flags() {
    let inventory = inventory();

    for flag in MULTIVALUE_FLAGS {
        assert!(
            inventory.contains(flag),
            "contract-inventory.md does not mention the multivalue flag `{flag}`"
        );
    }
}

#[test]
fn inventory_records_the_environment_variables() {
    let inventory = inventory();

    for name in [ENV_ORG, ENV_PAT, ENV_SERVER] {
        assert!(
            inventory.contains(name),
            "contract-inventory.md does not mention the environment variable `{name}`"
        );
    }
}

#[test]
fn inventory_records_the_api_version() {
    assert_eq!(API_VERSION, "7.1");

    let inventory = inventory();
    let expected = format!("api-version={API_VERSION}");

    assert!(
        inventory.contains(&expected),
        "contract-inventory.md does not mention the default `{expected}`"
    );
}
