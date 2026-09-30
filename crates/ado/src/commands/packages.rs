//! `ado packages list|versions|show` — the whole of
//! `lib/ado_cli/cli/packages.ex`: the Universal Package metadata surface under
//! `/{project}/_apis/packaging/feeds/{feed_id}/packages`, the two tables and the
//! detail view.
//!
//! The three commands take three positionals where every other area takes two
//! (`show` takes four: project, feed, package name, version), and `list` is the
//! one packages command with a query (`protocolType=UPack`). Each path segment is
//! percent-encoded strictly (D22), so a version like `1.0.0+build.5` — whose `+`
//! the frozen `URI.encode/1` leaves raw — cannot change the URL's structure.

use ado_core::client::encode_path_segment;
use ado_core::envelope::ok_value;
use ado_core::error::{AdoError, ErrorCode};
use serde_json::Value;

use crate::commands::items::items;
use crate::context::Context;
use crate::output::Report;

/// `ado packages list`: `GET …/feeds/{feed_id}/packages` with the module's one
/// param, `protocolType=UPack`. The value array unwraps to the value envelope;
/// the human path is the module's three-column table.
pub fn list(context: &mut Context, project: &str, feed_id: &str) -> Result<Report, AdoError> {
    let packages = items(context.client()?.list(
        &packages_path(project, feed_id),
        &[("protocolType".to_owned(), "UPack".to_owned())],
    )?);

    Ok(
        context.json_or_report(ok_value(Value::Array(packages.clone())), || {
            packages_table(&packages)
        }),
    )
}

/// `ado packages versions`: `GET …/packages/{package_name}/versions`, with no
/// params at all.
pub fn versions(
    context: &mut Context,
    project: &str,
    feed_id: &str,
    package_name: &str,
) -> Result<Report, AdoError> {
    let versions = items(context.client()?.list(
        &format!("{}/versions", versions_path(project, feed_id, package_name)),
        &[],
    )?);

    Ok(
        context.json_or_report(ok_value(Value::Array(versions.clone())), || {
            versions_table(&versions)
        }),
    )
}

/// `ado packages show`: `GET …/packages/{package_name}/versions/{package_version}`.
/// A 404 takes the module's own `Package '<name>@<version>' not found` wording
/// (D4's class).
pub fn show(
    context: &mut Context,
    project: &str,
    feed_id: &str,
    package_name: &str,
    package_version: &str,
) -> Result<Report, AdoError> {
    let path = version_path(project, feed_id, package_name, package_version);

    match context.client()?.get(&path, &[]) {
        Ok(package) => Ok(context.json_or_report(ok_value(package.clone()), || {
            Report::Text(package_detail(&package))
        })),
        Err(error) if error.code == ErrorCode::NotFound => Err(AdoError {
            message: format!("Package '{package_name}@{package_version}' not found"),
            ..error
        }),
        Err(error) => Err(error),
    }
}

/// The collection path: `/{project}/_apis/packaging/feeds/{feed_id}/packages`.
fn packages_path(project: &str, feed_id: &str) -> String {
    format!(
        "/{}/_apis/packaging/feeds/{}/packages",
        encode_path_segment(project),
        encode_path_segment(feed_id)
    )
}

fn versions_path(project: &str, feed_id: &str, package_name: &str) -> String {
    format!(
        "{}/{}",
        packages_path(project, feed_id),
        encode_path_segment(package_name)
    )
}

fn version_path(project: &str, feed_id: &str, package_name: &str, package_version: &str) -> String {
    format!(
        "{}/versions/{}",
        versions_path(project, feed_id, package_name),
        encode_path_segment(package_version)
    )
}

/// The module's `print_packages_table/1`: Name, Protocol, and the number of
/// `versions` the feed reported (an absent list counts zero).
fn packages_table(packages: &[Value]) -> Report {
    if packages.is_empty() {
        return Report::Text("No packages found.".to_owned());
    }

    let rows = packages
        .iter()
        .map(|package| {
            vec![
                value_text(package.get("name")),
                value_text(package.get("protocolType")),
                package
                    .get("versions")
                    .and_then(Value::as_array)
                    .map_or(0, Vec::len)
                    .to_string(),
            ]
        })
        .collect();

    Report::Table {
        headers: vec![
            "Name".to_owned(),
            "Protocol".to_owned(),
            "Versions".to_owned(),
        ],
        rows,
    }
}

/// The module's `print_versions_table/1`, whose Status column is
/// `(v["isLatest"] == true && "latest") || (v["isDeleted"] == true && "deleted") || "normal"`.
fn versions_table(versions: &[Value]) -> Report {
    if versions.is_empty() {
        return Report::Text("No versions found.".to_owned());
    }

    let rows = versions
        .iter()
        .map(|version| {
            vec![
                value_text(version.get("version")),
                version_status(version).to_owned(),
                value_text(version.get("publishDate")),
            ]
        })
        .collect();

    Report::Table {
        headers: vec![
            "Version".to_owned(),
            "Status".to_owned(),
            "Publish Date".to_owned(),
        ],
        rows,
    }
}

/// The module's `print_package_detail/1`, with its `Published:` spelling (no
/// space after the colon) and its `?` fallback for an absent size.
fn package_detail(package: &Value) -> String {
    let mut detail = String::from("\nPackage Details\n\n");

    detail.push_str(&"-".repeat(60));
    detail.push('\n');
    detail.push_str(&format!(
        "  Name:     {}\n",
        value_text(package.get("name"))
    ));
    detail.push_str(&format!(
        "  Version:  {}\n",
        value_text(package.get("version"))
    ));
    detail.push_str(&format!(
        "  Protocol: {}\n",
        value_text(package.get("protocolType"))
    ));
    detail.push_str(&format!("  Status:   {}\n", detail_status(package)));
    detail.push_str(&format!("  Size:     {}\n", size_text(package.get("size"))));
    detail.push_str(&format!(
        "  Published:{}\n",
        value_text(package.get("publishDate"))
    ));
    detail.push('\n');

    detail
}

/// The table's Status: `isLatest` first, then `isDeleted`, then `normal` — a
/// truthy-but-not-`true` value is none of them, exactly as `== true` says.
fn version_status(version: &Value) -> &'static str {
    if version.get("isLatest") == Some(&Value::Bool(true)) {
        "latest"
    } else if version.get("isDeleted") == Some(&Value::Bool(true)) {
        "deleted"
    } else {
        "normal"
    }
}

/// The detail's Status: only `isLatest` is consulted (`… || "normal"`).
fn detail_status(package: &Value) -> &'static str {
    if package.get("isLatest") == Some(&Value::Bool(true)) {
        "latest"
    } else {
        "normal"
    }
}

/// The module's `pkg["size"] || "?"`: nil and `false` fall back, `0` is a value.
fn size_text(size: Option<&Value>) -> String {
    match size {
        None | Some(Value::Null) | Some(Value::Bool(false)) => "?".to_owned(),
        Some(value) => value_text(Some(value)),
    }
}

/// Elixir's `#{term}` interpolation for the JSON scalars these fields carry:
/// strings verbatim, numbers and booleans as they print, and `nil`/absent as the
/// empty string.
fn value_text(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(text)) => text.clone(),
        Some(Value::Bool(flag)) => flag.to_string(),
        Some(Value::Number(number)) => number.to_string(),
        Some(other) => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_paths_encode_each_segment_strictly() {
        assert_eq!(
            packages_path("Alpha", "feed-1"),
            "/Alpha/_apis/packaging/feeds/feed-1/packages"
        );
        assert_eq!(
            version_path("Alpha", "feed-1", "myapp+builds", "1.0.0+build.5"),
            "/Alpha/_apis/packaging/feeds/feed-1/packages/myapp%2Bbuilds/versions/1.0.0%2Bbuild.5",
            "a `+` cannot reach the URL raw (D22)"
        );
    }

    #[test]
    fn the_status_labels_follow_the_two_expressions() {
        assert_eq!(version_status(&json!({"isLatest": true})), "latest");
        assert_eq!(version_status(&json!({"isDeleted": true})), "deleted");
        assert_eq!(version_status(&json!({"isLatest": false})), "normal");
        assert_eq!(
            version_status(&json!({"isLatest": true, "isDeleted": true})),
            "latest",
            "the table's `isLatest` wins"
        );
        assert_eq!(detail_status(&json!({"isDeleted": true})), "normal");
    }

    #[test]
    fn the_tables_name_every_row_and_count_the_versions() {
        let packages = vec![
            json!({"name": "myapp-builds", "protocolType": "UPack", "versions": [1, 2, 3]}),
            json!({"name": "other-pkg", "protocolType": "UPack"}),
        ];

        assert_eq!(
            packages_table(&packages),
            Report::Table {
                headers: vec![
                    "Name".to_owned(),
                    "Protocol".to_owned(),
                    "Versions".to_owned()
                ],
                rows: vec![
                    vec![
                        "myapp-builds".to_owned(),
                        "UPack".to_owned(),
                        "3".to_owned()
                    ],
                    vec!["other-pkg".to_owned(), "UPack".to_owned(), "0".to_owned()],
                ],
            }
        );
        assert_eq!(
            packages_table(&[]),
            Report::Text("No packages found.".to_owned())
        );
    }

    #[test]
    fn the_detail_carries_the_module_spellings_and_the_size_fallback() {
        let detail = package_detail(&json!({
            "name": "myapp-builds",
            "version": "1.0.0",
            "protocolType": "UPack",
            "size": 0,
        }));

        assert!(
            detail.contains("  Size:     0\n"),
            "size 0 is a value: {detail:?}"
        );
        assert!(
            detail.contains("  Published:\n"),
            "an absent date prints empty after the colon: {detail:?}"
        );
        assert!(
            detail.starts_with(&format!("\nPackage Details\n\n{}", "-".repeat(60))),
            "the detail is the module's ASCII rule, not box drawing: {detail:?}"
        );
    }

    #[test]
    fn an_absent_size_falls_back_to_the_question_mark() {
        assert_eq!(size_text(None), "?");
        assert_eq!(size_text(Some(&Value::Bool(false))), "?");
        assert_eq!(size_text(Some(&json!(2048))), "2048");
    }
}
