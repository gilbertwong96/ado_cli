//! `ado whoami` — the current authentication status, never the token.

use ado_core::envelope::ok_value;
use ado_core::error::AdoError;
use serde_json::{Value, json};

use crate::context::{AuthStatus, Context, DEFAULT_SERVER};
use crate::output::Report;

pub fn run(context: &Context) -> Result<Report, AdoError> {
    let status = context.auth_status();

    if context.json() {
        Ok(Report::Json(ok_value(status_json(&status))))
    } else {
        Ok(Report::Text(status_plain(&status)))
    }
}

fn status_json(status: &AuthStatus) -> Value {
    json!({
        "server": status.server,
        "org": status.org,
        "method": status.method,
        "configured": status.configured,
        "authenticated": status.authenticated,
        "config_file": config_file(status),
    })
}

fn status_plain(status: &AuthStatus) -> String {
    if status.authenticated {
        authenticated_plain(status)
    } else {
        unauthenticated_plain(status)
    }
}

fn authenticated_plain(status: &AuthStatus) -> String {
    format!(
        "\n  Organization: {}\n  Server:       {}\n  Auth Method:  {}\n  Config File:  {}\n\n",
        status.org.as_deref().unwrap_or("(not set)"),
        server_label(status),
        status.method.as_deref().unwrap_or("none"),
        config_file(status).unwrap_or_else(|| "(not set)".to_owned()),
    )
}

fn unauthenticated_plain(status: &AuthStatus) -> String {
    format!(
        "\n  Server:       {}\n  Not authenticated.\n\n  Authenticate with:\n    ado login --method pat --org ORG --pat TOKEN\n    ado login --method device --org ORG\n  Or set environment variables: ADO_ORG + ADO_PAT\n\n",
        server_label(status),
    )
}

/// The oracle prints `dev.azure.com (cloud)` when no server was configured; the
/// envelope carries the bare default.
fn server_label(status: &AuthStatus) -> String {
    if status.server == DEFAULT_SERVER {
        format!("{DEFAULT_SERVER} (cloud)")
    } else {
        status.server.clone()
    }
}

fn config_file(status: &AuthStatus) -> Option<String> {
    status
        .config_file
        .as_ref()
        .map(|path| path.display().to_string())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn status(config_file: Option<PathBuf>) -> AuthStatus {
        AuthStatus {
            configured: false,
            org: None,
            server: DEFAULT_SERVER.to_owned(),
            method: None,
            authenticated: false,
            config_file,
        }
    }

    #[test]
    fn json_without_a_config_path_reports_null() {
        assert_eq!(
            status_json(&status(None)),
            json!({
                "server": "dev.azure.com",
                "org": null,
                "method": null,
                "configured": false,
                "authenticated": false,
                "config_file": null,
            })
        );
    }

    #[test]
    fn plain_without_a_config_path_says_not_set() {
        let authenticated = AuthStatus {
            configured: true,
            org: Some("myorg".to_owned()),
            method: Some("pat".to_owned()),
            authenticated: true,
            ..status(None)
        };

        assert!(
            status_plain(&authenticated).contains("  Config File:  (not set)\n"),
            "plain output: {:?}",
            status_plain(&authenticated)
        );
    }
}
