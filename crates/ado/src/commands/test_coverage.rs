//! `ado test-coverage show` — the whole of `lib/ado_cli/cli/test_coverage.ex`:
//! the `_apis/test/codecoverage` read for one build, its three outcomes (data,
//! an empty `coverageData`, a body without the key) and the bar chart.
//!
//! The module's own `--json` path is the `coverageData` array under
//! `Output.ok/4`; its "no coverage data" branch writes human text in both modes,
//! which this build repairs to the empty value envelope (D21's rule for a read
//! whose frozen `--json` is prose) and keeps the module's human wording in human
//! mode. The chart's colours are the module's own hardcoded three (§8 leaves
//! colour free; the harness's text mode strips it either way).

use ado_core::client::encode_path_segment;
use ado_core::envelope::ok_value;
use ado_core::error::AdoError;
use serde_json::Value;

use crate::context::Context;
use crate::output::Report;

/// The 20 cells the module's bar is built from, and the percentage each cell
/// stands for.
const BAR_CELLS: usize = 20;

/// `ado test-coverage show PROJECT BUILD_ID`: `GET /{project}/_apis/test/codecoverage`
/// with the module's `buildId` pair.
pub fn show(context: &mut Context, project: &str, build_id: i64) -> Result<Report, AdoError> {
    let body = context
        .client()?
        .get(&coverage_path(project), &build_id_params(build_id))?;

    match body.get("coverageData") {
        Some(coverage) => {
            let data = coverage.as_array().cloned().unwrap_or_default();

            Ok(
                context.json_or_report(ok_value(Value::Array(data.clone())), || {
                    Report::Text(coverage_chart(build_id, &data))
                }),
            )
        }
        None => Ok(
            context.json_or_report(ok_value(Value::Array(Vec::new())), || {
                Report::Text(no_coverage_text(build_id))
            }),
        ),
    }
}

/// The coverage read of one project; the frozen module interpolates the project
/// raw and this build escapes the segment strictly (D22).
fn coverage_path(project: &str) -> String {
    format!("/{}/_apis/test/codecoverage", encode_path_segment(project))
}

/// The module's `%{"buildId" => build_id}`; the client adds `api-version`.
fn build_id_params(build_id: i64) -> Vec<(String, String)> {
    vec![("buildId".to_owned(), build_id.to_string())]
}

/// The module's `show_no_coverage/1`: the sentence, then its `halt_success/1`
/// message.
fn no_coverage_text(build_id: i64) -> String {
    format!("\nNo coverage data for build #{build_id}.\n\nDone.")
}

/// The module's `print_coverage_stats/1` over every configuration: a 70-cell
/// rule under one header, then one line per stat.
fn coverage_chart(build_id: i64, covers: &[Value]) -> String {
    let mut text = format!(
        "\nCode Coverage for Build #{build_id}\n{}\n",
        "─".repeat(70)
    );

    for cover in covers {
        for stat in cover
            .get("coverageStats")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            text.push_str(&coverage_line(stat));
            text.push('\n');
        }
    }

    text.push('\n');

    text
}

/// One stat line: the label padded to the module's 20 columns, the percentage
/// right-justified in eight, then the coloured bar.
fn coverage_line(stat: &Value) -> String {
    let label = match stat.get("label") {
        None | Some(Value::Null) | Some(Value::Bool(false)) => "?".to_owned(),
        Some(value) => value_text(Some(value)),
    };
    let total = number(stat.get("total"));
    let covered = number(stat.get("covered"));
    let pct = if total > 0.0 {
        round1(covered / total * 100.0)
    } else {
        0.0
    };

    format!(
        "  {label:<20} {:>8} {}",
        format!("{pct:.1}%"),
        coverage_bar(pct)
    )
}

/// `Float.round(value, 1)`: scale, round half away from zero, scale back — which
/// is what `:erlang.round/1` does, where Rust's decimal formatter rounds ties to
/// even.
fn round1(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

/// `s["total"] || 0` for the arithmetic: a missing, null or `false` value is
/// zero, and anything that is not a number reads as zero rather than raising.
fn number(value: Option<&Value>) -> f64 {
    match value {
        Some(Value::Number(number)) => number.as_f64().unwrap_or(0.0),
        _ => 0.0,
    }
}

/// The module's `coverage_bar/1`: `round(pct / 5)` filled cells and the rest
/// empty, wrapped in green at 80% or more, yellow at 50%, red below.
fn coverage_bar(pct: f64) -> String {
    let filled = (pct / 5.0).round().clamp(0.0, BAR_CELLS as f64) as usize;
    let bar = format!("{}{}", "█".repeat(filled), "░".repeat(BAR_CELLS - filled));
    let colour = if pct >= 80.0 {
        "\u{1b}[32m"
    } else if pct >= 50.0 {
        "\u{1b}[33m"
    } else {
        "\u{1b}[31m"
    };

    format!("{colour}{bar}\u{1b}[0m")
}

/// Elixir's `#{term}` interpolation for the JSON scalars these fields carry.
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
    fn the_path_encodes_the_project_as_one_segment() {
        assert_eq!(coverage_path("Alpha"), "/Alpha/_apis/test/codecoverage");
        assert_eq!(
            coverage_path("Alpha/Beta"),
            "/Alpha%2FBeta/_apis/test/codecoverage",
            "the frozen path interpolates the project raw (D22)"
        );
    }

    #[test]
    fn the_params_carry_the_build_id() {
        assert_eq!(
            build_id_params(42),
            vec![("buildId".to_owned(), "42".to_owned())]
        );
    }

    #[test]
    fn the_chart_draws_one_line_per_stat_with_the_modules_colours() {
        let chart = coverage_chart(
            42,
            &[json!({"coverageStats": [
                {"label": "Lines", "total": 100, "covered": 85},
                {"label": "Branches", "total": 40, "covered": 12},
                {"label": "Functions", "total": 100, "covered": 55},
            ]})],
        );
        let expected = format!(
            concat!(
                "\nCode Coverage for Build #42\n",
                "{}\n",
                "  Lines                   85.0% \u{1b}[32m{}\u{1b}[0m\n",
                "  Branches                30.0% \u{1b}[31m{}\u{1b}[0m\n",
                "  Functions               55.0% \u{1b}[33m{}\u{1b}[0m\n",
                "\n",
            ),
            "─".repeat(70),
            "█".repeat(17) + &"░".repeat(3),
            "█".repeat(6) + &"░".repeat(14),
            "█".repeat(11) + &"░".repeat(9),
        );

        assert_eq!(chart, expected);
    }

    #[test]
    fn an_empty_coverage_array_still_draws_the_header() {
        let chart = coverage_chart(42, &[]);

        assert_eq!(
            chart,
            format!("\nCode Coverage for Build #42\n{}\n\n", "─".repeat(70))
        );
    }

    #[test]
    fn a_stat_without_a_label_reads_a_question_mark_and_a_zero_total_is_an_empty_bar() {
        let chart = coverage_chart(7, &[json!({"coverageStats": [{"total": 0, "covered": 0}]})]);

        assert_eq!(
            chart,
            format!(
                "\nCode Coverage for Build #7\n{}\n  ?                        0.0% \u{1b}[31m{}\u{1b}[0m\n\n",
                "─".repeat(70),
                "░".repeat(20),
            )
        );
    }

    #[test]
    fn the_percentage_rounds_like_elixirs_float_round() {
        assert_eq!(round1(0.25), 0.3, "half away from zero, not ties-to-even");
        assert_eq!(round1(85.0), 85.0);
        assert_eq!(round1(0.04), 0.0);
    }

    #[test]
    fn the_bar_clamps_a_percentage_above_a_hundred_to_the_full_width() {
        assert_eq!(
            coverage_bar(120.0),
            format!("\u{1b}[32m{}\u{1b}[0m", "█".repeat(20)),
            "20 cells, never a negative repeat"
        );
    }

    #[test]
    fn the_no_coverage_text_keeps_the_modules_three_lines() {
        assert_eq!(
            no_coverage_text(43),
            "\nNo coverage data for build #43.\n\nDone."
        );
    }
}
