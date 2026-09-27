use ado_core::envelope::ok_named;
use serde_json::json;

use crate::VERSION;
use crate::output::Report;

pub fn run(json: bool) -> Report {
    if json {
        Report::Json(ok_named("version", json!(VERSION)))
    } else {
        Report::Text(format!("ado {VERSION}"))
    }
}
