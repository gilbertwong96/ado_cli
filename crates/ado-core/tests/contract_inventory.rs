//! The contract inventory checked against the code it records: the nine error
//! codes, the live exit-code contract (spec §6.2–6.3), and ruling R23.

use std::fs;
use std::path::PathBuf;

use ado_core::error::{ErrorCode, exit_code_for};

fn inventory_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/rust-rewrite/contract-inventory.md")
}

fn inventory() -> String {
    let path = inventory_path();

    fs::read_to_string(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

#[test]
fn inventory_records_every_error_code_and_the_exit_contract() {
    let inventory = inventory();

    for code in ErrorCode::ALL {
        let code = code.as_str();

        assert!(
            inventory.contains(code),
            "contract-inventory.md does not mention the error code `{code}`"
        );
    }

    assert!(
        inventory.contains("exit 1"),
        "contract-inventory.md does not state the live exit-code contract (`exit 1`)"
    );
}

/// R23: the exit-code contract is pinned by calling `exit_code_for`, not by
/// reading its definition — every code exits 1.
#[test]
fn every_error_code_exits_one() {
    for code in ErrorCode::ALL {
        assert_eq!(exit_code_for(&code), 1, "`{}` must exit 1", code.as_str());
    }
}
