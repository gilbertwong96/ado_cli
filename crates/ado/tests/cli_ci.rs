//! End-to-end tests for `ado ci watch`: the poll loop's request chain, the log
//! stream's `?id=N` progression, the `--latest` resolution, the documented
//! `--poll-interval` clamp and the repaired 0/1/2 exit statuses (Rulings 4(a) and
//! 3).
//!
//! Every test owns its environment: a `TempHome` for the config directory, a
//! `StandaloneMock` behind `ADO_SERVER` with the scripted `sequence` responses the
//! loop needs, and explicit `ADO_ORG`/`ADO_PAT` credentials, so no credential
//! resolution reaches the developer's keychain. Stdin is null in every spawn, so
//! no test can read a terminal.

use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

use ado_testkit::{
    RecordedRequest, Scenario, StandaloneMock, TempHome, ado_cmd, stderr_of, stdout_of,
};
use serde_json::{Value, json};

const ORG: &str = "myorg";

/// A completed build with `result`, as the mock serves it.
fn terminal(id: i64, result: &str) -> Value {
    json!({
        "id": id,
        "status": "completed",
        "result": result,
        "sourceBranch": "refs/heads/main",
        "definition": {"name": "Alpha CI"},
    })
}

/// A completed, succeeded build, as the mock serves it.
fn completed(result: &str) -> Value {
    terminal(123, result)
}

/// A timeline whose one record is an in-progress job logging to log 9.
fn log_timeline() -> Value {
    timeline(json!([
        {"id": "job-1", "state": "inProgress", "type": "Job", "name": "Build", "log": {"id": 9}}
    ]))
}

fn running(id: i64) -> Value {
    json!({
        "id": id,
        "status": "inProgress",
        "result": null,
        "sourceBranch": "refs/heads/release",
        "definition": {"name": "Alpha CI"},
    })
}

fn timeline(records: Value) -> Value {
    json!({"records": records})
}

/// A route table from a scenario document; the mock serves each route's
/// `sequence` entries one per matching request, the last repeating.
fn mock(document: Value) -> StandaloneMock {
    let scenario = Scenario::from_json(&document.to_string()).expect("the test scenario parses");

    StandaloneMock::start(scenario, None, 0)
}

fn build_route(id: i64, sequence: Vec<Value>) -> Value {
    let entries = sequence
        .into_iter()
        .map(|json| json!({"json": json}))
        .collect::<Vec<_>>();

    json!({"method": "GET", "path": format!("/{ORG}/Alpha/_apis/build/builds/{id}"), "sequence": entries})
}

fn timeline_route(id: i64, body: Value) -> Value {
    json!({"method": "GET", "path": format!("/{ORG}/Alpha/_apis/build/builds/{id}/timeline"), "json": body})
}

fn command(home: &TempHome, server: &StandaloneMock, args: &[&str]) -> Command {
    let mut command = ado_cmd();
    home.apply(&mut command);
    command
        .env("ADO_ORG", ORG)
        .env("ADO_PAT", "test-pat")
        .env("ADO_SERVER", server.base_url())
        .args(args);
    command
}

fn run(home: &TempHome, server: &StandaloneMock, args: &[&str]) -> Output {
    command(home, server, args)
        .stdin(Stdio::null())
        .output()
        .expect("run ado")
}

fn assert_exit(output: &Output, expected: i32) {
    assert_eq!(
        output.status.code(),
        Some(expected),
        "stdout:\n{}\nstderr:\n{}",
        stdout_of(output),
        stderr_of(output)
    );
}

fn paths(requests: &[RecordedRequest]) -> Vec<&str> {
    requests
        .iter()
        .map(|request| request.path.as_str())
        .collect()
}

/// The two-request chain a completed build walks.
fn completed_scenario(id: i64, result: &str) -> Value {
    json!({"responses": [
        build_route(id, vec![completed(result)]),
        timeline_route(id, timeline(json!([
            {"id": "rec-1", "state": "completed", "type": "Job", "name": "Compile"}
        ]))),
    ]})
}

/// A scenario whose only route no case in the no-request tests can reach; the
/// standalone mock refuses an empty route table, and these tests exist to prove
/// nothing is sent.
fn unreachable_scenario() -> Value {
    json!({"responses": [
        {"method": "GET", "path": "/unreachable", "json": {"ok": true}}
    ]})
}

#[test]
fn a_completed_build_polls_once_and_prints_the_frozen_lines() {
    let home = TempHome::new();
    let server = mock(completed_scenario(123, "succeeded"));

    let output = run(&home, &server, &["ci", "watch", "Alpha", "123"]);

    assert_exit(&output, 0);
    assert_eq!(
        stdout_of(&output),
        concat!(
            "✓ Build 123 · Alpha CI · refs/heads/main · succeeded in <1s\n",
            "    ✔ [Job] Compile\n",
            "\n",
            "  Build succeeded.\n",
            "\n",
            "✓ Build 123 completed.\n",
        )
    );
    assert_eq!(stderr_of(&output), "");
    assert_eq!(
        paths(&server.received()),
        [
            "/myorg/Alpha/_apis/build/builds/123",
            "/myorg/Alpha/_apis/build/builds/123/timeline"
        ]
    );
}

#[test]
fn a_failed_build_exits_one_with_the_repaired_line() {
    let home = TempHome::new();
    let server = mock(completed_scenario(123, "failed"));

    let output = run(&home, &server, &["ci", "watch", "Alpha", "123"]);

    assert_exit(&output, 1);
    assert!(
        stdout_of(&output).ends_with("\n  Build failed.\n\n✗ Build 123 failed.\n"),
        "stdout:\n{}",
        stdout_of(&output)
    );
}

#[test]
fn a_canceled_build_exits_two() {
    let home = TempHome::new();
    let server = mock(completed_scenario(123, "canceled"));

    let output = run(&home, &server, &["ci", "watch", "Alpha", "123"]);

    assert_exit(&output, 2);
    assert!(
        stdout_of(&output).ends_with("\n  Build was canceled.\n\n✗ Build 123 canceled.\n"),
        "stdout:\n{}",
        stdout_of(&output)
    );
}

#[test]
fn a_cancelling_build_is_terminal_and_exits_two() {
    let home = TempHome::new();
    let server = mock(json!({"responses": [
        build_route(123, vec![json!({"id": 123, "status": "cancelling"})]),
        timeline_route(123, timeline(json!([]))),
    ]}));

    let output = run(&home, &server, &["ci", "watch", "Alpha", "123"]);

    assert_exit(&output, 2);
    assert!(
        stdout_of(&output).contains("* Build 123 · cancelling… (<1s)"),
        "stdout:\n{}",
        stdout_of(&output)
    );
    assert!(
        stdout_of(&output).ends_with("✗ Build 123 canceled.\n"),
        "stdout:\n{}",
        stdout_of(&output)
    );
}

#[test]
fn a_partially_succeeded_build_exits_zero() {
    let home = TempHome::new();
    let server = mock(completed_scenario(123, "partiallySucceeded"));

    let output = run(&home, &server, &["ci", "watch", "Alpha", "123"]);

    assert_exit(&output, 0);
    assert!(
        stdout_of(&output).contains("\n  Build partially succeeded (some warnings).\n"),
        "stdout:\n{}",
        stdout_of(&output)
    );
    assert!(
        stdout_of(&output).ends_with("✓ Build 123 completed.\n"),
        "stdout:\n{}",
        stdout_of(&output)
    );
}

#[test]
fn the_two_tick_sequence_records_the_frozen_chain() {
    let home = TempHome::new();
    let server = mock(json!({"responses": [
        build_route(127, vec![running(127), completed("succeeded")]),
        timeline_route(127, timeline(json!([
            {"id": "rec-1", "state": "inProgress", "type": "Job", "name": "Compile"}
        ]))),
    ]}));

    let output = run(
        &home,
        &server,
        &["ci", "watch", "Alpha", "127", "--poll-interval", "250"],
    );

    assert_exit(&output, 0);
    // The frozen chain: two timeline fetches on the non-terminal tick and one on
    // the terminal one — the same path twice, which only the request log sees.
    assert_eq!(
        paths(&server.received()),
        [
            "/myorg/Alpha/_apis/build/builds/127",
            "/myorg/Alpha/_apis/build/builds/127/timeline",
            "/myorg/Alpha/_apis/build/builds/127/timeline",
            "/myorg/Alpha/_apis/build/builds/127",
            "/myorg/Alpha/_apis/build/builds/127/timeline"
        ]
    );
    let stdout = stdout_of(&output);
    assert!(
        stdout.starts_with("* Build 127 · Alpha CI · refs/heads/release · running for <1s\n"),
        "stdout:\n{stdout}"
    );
    // The frozen loop discards `render_timeline_diff`'s state update, so the
    // record re-prints on the terminal tick (captured `w8-two-tick`).
    assert_eq!(
        stdout.matches("    * [Job] Compile\n").count(),
        2,
        "stdout:\n{stdout}"
    );
    assert!(
        stdout.ends_with("✓ Build 127 completed.\n"),
        "stdout:\n{stdout}"
    );
}

#[test]
fn the_log_stream_advances_id_and_prints_each_part_once() {
    let home = TempHome::new();
    let server = mock(json!({"responses": [
        build_route(128, vec![running(128), running(128), json!({
            "id": 128, "status": "completed", "result": "failed",
            "sourceBranch": "refs/heads/main", "definition": {"name": "Alpha CI"}
        })]),
        timeline_route(128, timeline(json!([
            {"id": "job-1", "state": "inProgress", "type": "Job", "name": "Build", "log": {"id": 7}},
            {"id": "job-2", "state": "completed", "type": "Job", "name": "Setup"}
        ]))),
        json!({"method": "GET", "path": format!("/{ORG}/Alpha/_apis/build/builds/128/logs/7"), "sequence": [
            {"fixture": "build_log_part_one.txt"},
            {"fixture": "build_log_part_two.txt"}
        ]}),
    ]}));

    let output = run(
        &home,
        &server,
        &["ci", "watch", "Alpha", "128", "--poll-interval", "250"],
    );

    assert_exit(&output, 1);
    let stdout = stdout_of(&output);
    assert!(stdout.contains("Step A\nStep B\n"), "stdout:\n{stdout}");
    assert!(stdout.contains("Step C\n"), "stdout:\n{stdout}");
    assert_eq!(stdout.matches("Step A").count(), 1, "stdout:\n{stdout}");
    assert!(
        stdout.ends_with("✗ Build 128 failed.\n"),
        "stdout:\n{stdout}"
    );

    let requests = server.received();
    assert_eq!(
        paths(&requests).len(),
        10,
        "the frozen nine-request chain plus the last log"
    );
    let log_requests = requests
        .iter()
        .filter(|request| request.path.ends_with("/logs/7"))
        .collect::<Vec<_>>();
    assert_eq!(log_requests.len(), 2, "one log fetch per non-terminal tick");
    assert_eq!(
        log_requests[0].query_pairs(),
        [
            ("api-version".to_owned(), "7.1".to_owned()),
            ("id".to_owned(), "1".to_owned())
        ],
        "the first poll asks for line 1"
    );
    assert_eq!(
        log_requests[1].query_pairs(),
        [
            ("api-version".to_owned(), "7.1".to_owned()),
            ("id".to_owned(), "3".to_owned())
        ],
        "two CRLF lines advance the cursor to 3"
    );
}

#[test]
fn the_poll_interval_is_accepted_and_used() {
    let home = TempHome::new();
    let server = mock(json!({"responses": [
        build_route(127, vec![running(127), completed("succeeded")]),
        timeline_route(127, timeline(json!([]))),
    ]}));

    let started = Instant::now();
    let output = run(
        &home,
        &server,
        &["ci", "watch", "Alpha", "127", "--poll-interval", "250"],
    );
    let elapsed = started.elapsed();

    assert_exit(&output, 0);
    assert!(
        elapsed < Duration::from_millis(1_600),
        "the interval is used, so two 250 ms ticks finish well inside the default's \
         single tick; took {elapsed:?}"
    );
}

#[test]
fn the_poll_interval_is_clamped_like_the_help_says() {
    let home = TempHome::new();
    let server = mock(completed_scenario(123, "succeeded"));

    // 100 is below the floor: the documented clamp is the 2000 ms default, so the
    // single-tick watch still completes — the unit tests pin the arithmetic.
    let output = run(
        &home,
        &server,
        &["ci", "watch", "Alpha", "123", "--poll-interval", "100"],
    );

    assert_exit(&output, 0);
    assert!(stdout_of(&output).ends_with("✓ Build 123 completed.\n"));
}

#[test]
fn latest_resolves_the_first_build_and_sends_the_filters() {
    let home = TempHome::new();
    let server = mock(json!({"responses": [
        json!({"method": "GET", "path": format!("/{ORG}/Alpha/_apis/build/builds"),
               "json": {"count": 1, "value": [{"id": 123, "status": "completed"}]}}),
        build_route(123, vec![completed("succeeded")]),
        timeline_route(123, timeline(json!([]))),
    ]}));

    let output = run(
        &home,
        &server,
        &[
            "ci",
            "watch",
            "Alpha",
            "--latest",
            "--definition",
            "7",
            "--branch",
            "refs/heads/main",
        ],
    );

    assert_exit(&output, 0);
    assert!(stdout_of(&output).ends_with("✓ Build 123 completed.\n"));

    let requests = server.received();
    assert_eq!(paths(&requests)[0], "/myorg/Alpha/_apis/build/builds");
    assert_eq!(
        requests[0].query_pairs(),
        [
            ("api-version".to_owned(), "7.1".to_owned()),
            ("%24top".to_owned(), "1".to_owned()),
            ("definitions".to_owned(), "7".to_owned()),
            ("branchName".to_owned(), "refs%2Fheads%2Fmain".to_owned())
        ],
        "the version first (the client's merge), then the module's pairs in its order"
    );
}

#[test]
fn latest_with_no_builds_is_the_modules_sentence() {
    let home = TempHome::new();
    let server = mock(json!({"responses": [
        json!({"method": "GET", "path": format!("/{ORG}/Alpha/_apis/build/builds"),
               "json": {"count": 0, "value": []}}),
    ]}));

    let output = run(&home, &server, &["ci", "watch", "Alpha", "--latest"]);

    assert_exit(&output, 1);
    assert!(
        stderr_of(&output).contains("no builds found for this project/definition/branch"),
        "stderr:\n{}",
        stderr_of(&output)
    );
    assert_eq!(
        paths(&server.received()).len(),
        1,
        "only the collection request"
    );
}

#[test]
fn no_build_id_and_no_latest_is_the_modules_refusal() {
    let home = TempHome::new();
    let server = mock(unreachable_scenario());

    let output = run(&home, &server, &["ci", "watch", "Alpha"]);

    assert_exit(&output, 1);
    assert!(
        stderr_of(&output).contains(
            "no build ID given; pass BUILD_ID as the second positional argument or use --latest"
        ),
        "stderr:\n{}",
        stderr_of(&output)
    );
    assert!(
        server.received().is_empty(),
        "nothing is fetched without a build"
    );
}

#[test]
fn a_build_id_of_zero_and_a_negative_one_are_treated_as_missing() {
    let home = TempHome::new();

    for id in ["0", "-1"] {
        let server = mock(unreachable_scenario());
        let output = run(&home, &server, &["ci", "watch", "Alpha", id]);

        assert_exit(&output, 1);
        assert!(
            stderr_of(&output).contains("no build ID given"),
            "{id}: stderr:\n{}",
            stderr_of(&output)
        );
        assert!(server.received().is_empty(), "{id}: nothing is fetched");
    }
}

#[test]
fn the_watch_reports_a_missing_build() {
    let home = TempHome::new();
    let server = mock(json!({"responses": [
        json!({"method": "GET", "path": format!("/{ORG}/Alpha/_apis/build/builds/404"),
               "status": 404, "json": {"message": "Build 404 does not exist."}}),
    ]}));

    let output = run(&home, &server, &["ci", "watch", "Alpha", "404"]);

    assert_exit(&output, 1);
    assert!(
        stderr_of(&output).starts_with("[Not found] "),
        "stderr:\n{}",
        stderr_of(&output)
    );
    assert_eq!(paths(&server.received()).len(), 1);
}

#[test]
fn a_timeline_failure_is_swallowed_like_the_oracle() {
    let home = TempHome::new();
    // Two ticks, so both per-tick fetch sites meet the failing timeline: the
    // diff fetch on each tick and the log-stream fetch on the non-terminal one
    // (captured `w8-timeline-500`).
    let server = mock(json!({"responses": [
        build_route(127, vec![running(127), terminal(127, "succeeded")]),
        json!({"method": "GET", "path": format!("/{ORG}/Alpha/_apis/build/builds/127/timeline"),
               "status": 500, "json": {"message": "TF400813: The server is unavailable."}}),
    ]}));

    let output = run(
        &home,
        &server,
        &["ci", "watch", "Alpha", "127", "--poll-interval", "250"],
    );

    assert_exit(&output, 0);
    assert!(
        stdout_of(&output).ends_with("\n  Build succeeded.\n\n✓ Build 127 completed.\n"),
        "no timeline line, the final sentence still printed (captured `w8-timeline-500`):\n{}",
        stdout_of(&output)
    );
}

#[test]
fn latest_reports_a_malformed_collection() {
    let home = TempHome::new();

    for body in [
        json!({"count": 1, "value": [{"status": "completed"}]}),
        json!({"count": 0}),
    ] {
        let server = mock(json!({"responses": [
            json!({"method": "GET", "path": format!("/{ORG}/Alpha/_apis/build/builds"), "json": body}),
        ]}));

        let output = run(&home, &server, &["ci", "watch", "Alpha", "--latest"]);

        assert_exit(&output, 1);
        assert!(
            stderr_of(&output).contains("the builds endpoint answered an unexpected response"),
            "{body}: stderr:\n{}",
            stderr_of(&output)
        );
    }
}

#[test]
fn a_failed_log_fetch_keeps_the_cursor() {
    let home = TempHome::new();
    let server = mock(json!({"responses": [
        build_route(129, vec![running(129), running(129), terminal(129, "failed")]),
        timeline_route(129, log_timeline()),
        json!({"method": "GET", "path": format!("/{ORG}/Alpha/_apis/build/builds/129/logs/9"), "sequence": [
            {"status": 500, "json": {"message": "TF400813: The server is unavailable."}},
            {"fixture": "build_log_part_two.txt"}
        ]}),
    ]}));

    let output = run(
        &home,
        &server,
        &["ci", "watch", "Alpha", "129", "--poll-interval", "250"],
    );

    assert_exit(&output, 1);
    let logs = server
        .received()
        .into_iter()
        .filter(|request| request.path.ends_with("/logs/9"))
        .collect::<Vec<_>>();
    assert_eq!(logs.len(), 2, "one fetch per non-terminal tick");
    assert_eq!(
        logs[0].query_pairs(),
        [
            ("api-version".to_owned(), "7.1".to_owned()),
            ("id".to_owned(), "1".to_owned())
        ]
    );
    assert_eq!(
        logs[1].query_pairs(),
        [
            ("api-version".to_owned(), "7.1".to_owned()),
            ("id".to_owned(), "1".to_owned())
        ],
        "a failed fetch keeps the cursor, so the next tick retries the same range"
    );
    assert!(
        stdout_of(&output).contains("Step C\n"),
        "the retry's body prints:\n{}",
        stdout_of(&output)
    );
}

#[test]
fn an_empty_log_body_keeps_the_cursor() {
    let home = TempHome::new();
    let server = mock(json!({"responses": [
        build_route(130, vec![running(130), running(130), terminal(130, "succeeded")]),
        timeline_route(130, log_timeline()),
        json!({"method": "GET", "path": format!("/{ORG}/Alpha/_apis/build/builds/130/logs/9"), "sequence": [
            {"fixture": "build_log_empty.txt"},
            {"fixture": "build_log_part_two.txt"}
        ]}),
    ]}));

    let output = run(
        &home,
        &server,
        &["ci", "watch", "Alpha", "130", "--poll-interval", "250"],
    );

    assert_exit(&output, 0);
    let logs = server
        .received()
        .into_iter()
        .filter(|request| request.path.ends_with("/logs/9"))
        .collect::<Vec<_>>();
    assert_eq!(logs.len(), 2);
    assert_eq!(
        logs[1].query_pairs().get(1),
        Some(&("id".to_owned(), "1".to_owned())),
        "an empty body leaves the cursor where it was"
    );
    assert!(stdout_of(&output).contains("Step C\n"));
}

#[test]
fn json_mode_keeps_stdout_one_document() {
    let home = TempHome::new();
    let server = mock(completed_scenario(123, "succeeded"));

    let output = run(&home, &server, &["ci", "watch", "Alpha", "123", "--json"]);

    assert_exit(&output, 0);
    let document: Value = serde_json::from_str(&stdout_of(&output)).unwrap_or_else(|error| {
        panic!(
            "one JSON document ({error}); stdout:\n{}\nstderr:\n{}",
            stdout_of(&output),
            stderr_of(&output)
        )
    });
    assert_eq!(
        document,
        json!({"ok": true, "message": "✓ Build 123 completed."}),
        "the stream is suppressed under --json; the message envelope is the whole stdout"
    );
}

#[test]
fn json_mode_reports_the_repaired_failure_pair() {
    let home = TempHome::new();
    let server = mock(completed_scenario(123, "failed"));

    let output = run(&home, &server, &["ci", "watch", "Alpha", "123", "--json"]);

    assert_exit(&output, 1);
    let document: Value = serde_json::from_str(&stdout_of(&output)).unwrap_or_else(|error| {
        panic!(
            "one JSON document ({error}); stdout:\n{}\nstderr:\n{}",
            stdout_of(&output),
            stderr_of(&output)
        )
    });
    assert_eq!(
        document,
        json!({"ok": true, "message": "✗ Build 123 failed."}),
        "the command succeeded in observing a failure; the exit code carries the build"
    );
}

#[test]
fn an_interrupted_watch_exits_two() {
    let home = TempHome::new();
    let server = mock(json!({"responses": [
        build_route(134, vec![running(134)]),
        timeline_route(134, timeline(json!([]))),
    ]}));

    let mut child = command(
        &home,
        &server,
        &["ci", "watch", "Alpha", "134", "--poll-interval", "250"],
    )
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .expect("spawn ado");

    // The first request proves the loop is running, and with it the SIGINT
    // handler `watch` installs before its first fetch — a fixed sleep would race
    // the child's startup under a parallel test run.
    let started = Instant::now();
    while server.received().is_empty() {
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "the watch never sent its first request"
        );
        std::thread::sleep(Duration::from_millis(10));
    }

    let signal = Command::new("kill")
        .args(["-INT", &child.id().to_string()])
        .status()
        .expect("send SIGINT");
    assert!(signal.success(), "kill -INT failed");

    let output = wait_with_deadline(&mut child, Duration::from_secs(5));

    assert_exit(&output, 2);
    assert!(
        stdout_of(&output).starts_with("* Build 134 · "),
        "the stream before the signal:\n{}",
        stdout_of(&output)
    );
    assert!(
        stdout_of(&output).ends_with("\n✗ Watch cancelled.\n"),
        "stdout:\n{}",
        stdout_of(&output)
    );
}

#[test]
fn usage_errors_are_the_labelled_exit_one_lines() {
    let home = TempHome::new();
    let server = mock(unreachable_scenario());

    for (args, expected) in [
        (vec!["ci"], "missing sub-command"),
        (vec!["ci", "watch"], "required arguments were not provided"),
        (
            vec!["ci", "watch", "Alpha", "123", "456"],
            "unexpected argument '456'",
        ),
        (vec!["ci", "watch", "Alpha", "abc"], "invalid digit"),
    ] {
        let output = run(&home, &server, &args);

        assert_exit(&output, 1);
        assert!(
            stderr_of(&output).contains(expected),
            "{args:?}: stderr:\n{}",
            stderr_of(&output)
        );
    }

    assert!(server.received().is_empty(), "a usage error sends nothing");
}

/// Waits for a child with a deadline, so a watcher that never notices its signal
/// fails the test instead of hanging the suite.
fn wait_with_deadline(child: &mut Child, deadline: Duration) -> Output {
    let started = Instant::now();

    loop {
        if let Some(_status) = child.try_wait().expect("poll the child") {
            return collect(child);
        }

        assert!(
            started.elapsed() < deadline,
            "the watch did not stop within {deadline:?}"
        );

        std::thread::sleep(Duration::from_millis(20));
    }
}

fn collect(child: &mut Child) -> Output {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    use std::io::Read;
    if let Some(pipe) = child.stdout.as_mut() {
        pipe.read_to_end(&mut stdout).expect("read stdout");
    }
    if let Some(pipe) = child.stderr.as_mut() {
        pipe.read_to_end(&mut stderr).expect("read stderr");
    }

    Output {
        status: child.wait().expect("reap the child"),
        stdout,
        stderr,
    }
}
