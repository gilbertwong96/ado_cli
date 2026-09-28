//! The standalone mock the oracle harness starts as its own process:
//!
//! ```text
//! cargo build -p ado-testkit --bin mock
//! target/debug/mock --scenario scripts/oracle-mock-scenario.json \
//!     --record <file> [--port 0]
//! ```
//!
//! It prints its origin on stdout once it is listening, then serves the scenario
//! until it is killed. The harness points both CLIs at it with `ADO_SERVER` and
//! splits the request log by counting lines before and after each run.

use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::mpsc;

use ado_testkit::{Scenario, StandaloneMock};

fn main() -> ExitCode {
    let mut scenario = PathBuf::from("scripts/oracle-mock-scenario.json");
    let mut record = None;
    let mut port: u16 = 0;

    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        let value = match (flag.as_str(), args.next()) {
            ("--scenario", Some(value)) => value,
            ("--record", Some(value)) => {
                record = Some(PathBuf::from(value));
                continue;
            }
            ("--port", Some(value)) => {
                match value.parse() {
                    Ok(parsed) => port = parsed,
                    Err(error) => {
                        eprintln!("mock: --port {value} is not a port number: {error}");
                        return ExitCode::FAILURE;
                    }
                }
                continue;
            }
            _ => {
                eprintln!("mock: expected --scenario, --record or --port, got '{flag}'");
                return ExitCode::FAILURE;
            }
        };

        scenario = PathBuf::from(value);
    }

    let scenario = match Scenario::load(&scenario) {
        Ok(scenario) => scenario,
        Err(error) => {
            eprintln!("mock: {error}");
            return ExitCode::FAILURE;
        }
    };

    let mock = StandaloneMock::start(scenario, record.as_deref(), port);
    println!("{}", mock.base_url());
    let _ = std::io::stdout().flush();

    let (_never, alive) = mpsc::channel::<()>();
    let _ = alive.recv();

    ExitCode::SUCCESS
}
