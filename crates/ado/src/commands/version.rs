use ado_core::envelope::ok_named;
use ado_core::error::AdoError;
use serde_json::json;

use crate::VERSION;
use crate::output::Report;

pub fn run(json: bool) -> Result<Report, AdoError> {
    if json {
        Ok(Report::Json(ok_named("version", json!(VERSION))))
    } else {
        Ok(Report::Text(format!("ado {VERSION}")))
    }
}
