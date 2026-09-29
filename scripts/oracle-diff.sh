#!/usr/bin/env bash
# Wave 0 oracle diff — the frozen Elixir `ado` (the oracle) against the Rust
# release binary, for every command Wave 0 ported: `version`, `whoami`, `schema`
# and `completion`.
#
# `--mock` is the read-and-mutation mode: every Wave 1 command and every Wave 2
# mutation case, both binaries, one instance of the testkit's standalone mock with
# `ADO_SERVER` pointed at it. Its section below describes the comparison, the
# route-level body assertions and the stdin-driven prompt cases.
#
# Both sides run with an isolated HOME and XDG_CONFIG_HOME and with ADO_ORG,
# ADO_PAT and ADO_SERVER unset, so neither binary can read a developer's config
# or environment. JSON is normalised with `jq -S` before it is compared, because
# the Rust encoder sorts object keys and the Elixir one does not (D1). Nothing is
# compared byte for byte except the completion scripts, where the difference is
# itself the recorded result: help, plain wording and the generated completion
# scripts are regenerated surface (spec §8).
#
# Each case prints one verdict line:
#
#   MATCH          the two projections are equal
#   EXPECTED-DIFF  they differ, and contract-inventory §9/§10 records why
#   DIFF           they differ for a reason nothing records
#
# Exit status: 0 when every difference is recorded, 1 when at least one is not,
# 2 when the script cannot run (missing binary, mock or jq).
#
# Usage: scripts/oracle-diff.sh [--mock]
#   ADO_ORACLE_ELIXIR=<path>   override the oracle   (default ./ado)
#   ADO_ORACLE_RUST=<path>     override the candidate (default target/release/ado)
#   ADO_ORACLE_MOCK=<path>     override the mock     (default target/debug/mock)
#   ADO_ORACLE_SCENARIO=<path> override the mock's route table
#                              (default scripts/oracle-mock-scenario.json)

set -uo pipefail

root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
elixir_bin=${ADO_ORACLE_ELIXIR:-$root/ado}
rust_bin=${ADO_ORACLE_RUST:-$root/target/release/ado}

mode=wave0
case ${1:-} in
    --mock) mode=mock ;;
    "") ;;
    *)
        printf 'usage: %s [--mock]\n' "${BASH_SOURCE[0]}" >&2
        exit 2
        ;;
esac

for binary in "$elixir_bin" "$rust_bin"; do
    if [[ ! -x $binary ]]; then
        printf 'oracle-diff: %s is missing or not executable\n' "$binary" >&2
        printf 'oracle-diff: build the oracle (`mix escript.build`) and the release binary (`cargo build --release`) first\n' >&2
        exit 2
    fi
done

if ! command -v jq >/dev/null 2>&1; then
    printf 'oracle-diff: jq is required\n' >&2
    exit 2
fi

scratch=${TMPDIR:-/tmp}
scratch=${scratch%/}
work=$(mktemp -d "$scratch/ado-oracle-diff.XXXXXX")
mock_pid=
trap 'stop_mock; rm -rf "$work"' EXIT

mkdir -p "$work/home-elixir" "$work/home-rust"

# ── the two sides ────────────────────────────────────────────────────────

oracle() {
    env -u ADO_ORG -u ADO_PAT -u ADO_SERVER \
        HOME="$work/home-elixir" XDG_CONFIG_HOME="$work/home-elixir/config" \
        "$elixir_bin" "$@"
}

candidate() {
    env -u ADO_ORG -u ADO_PAT -u ADO_SERVER \
        HOME="$work/home-rust" XDG_CONFIG_HOME="$work/home-rust/config" \
        "$rust_bin" "$@"
}

# Runs one case on both sides: <name> <args...> writes <name>.elixir/<name>.rust
# (stdout), their .err and their .status.
capture() {
    local name=$1
    shift
    oracle "$@" >"$work/$name.elixir" 2>"$work/$name.elixir.err"
    printf '%s' "$?" >"$work/$name.elixir.status"
    candidate "$@" >"$work/$name.rust" 2>"$work/$name.rust.err"
    printf '%s' "$?" >"$work/$name.rust.status"
}

# ── verdicts ─────────────────────────────────────────────────────────────

matches=0
expected=0
differences=0

start_case() {
    case_label=$1
    case_state=MATCH
    case_notes=()
}

# An unrecorded difference: the case fails.
fail() {
    case_state=DIFF
    case_notes+=("$1")
}

# A difference contract-inventory §9/§10 records.
ruled() {
    [[ $case_state == DIFF ]] || case_state=EXPECTED-DIFF
    case_notes+=("$1")
}

note() {
    case_notes+=("$1")
}

finish_case() {
    local detail=""
    if (( ${#case_notes[@]} > 0 )); then
        printf -v detail '%s; ' "${case_notes[@]}"
        detail=${detail%; }
    fi

    printf '%-14s %-22s %s\n' "$case_state" "$case_label" "$detail"

    case $case_state in
        MATCH) matches=$((matches + 1)) ;;
        EXPECTED-DIFF) expected=$((expected + 1)) ;;
        DIFF) differences=$((differences + 1)) ;;
    esac
}

summary() {
    printf '\noracle-diff: %d cases — %d match, %d expected-diff, %d unrecorded diff\n' \
        "$((matches + expected + differences))" "$matches" "$expected" "$differences"

    if (( differences > 0 )); then
        printf 'oracle-diff: FAIL — %d unrecorded difference(s); update docs/rust-rewrite/contract-inventory.md or fix the drift\n' "$differences" >&2
        exit 1
    fi

    printf 'oracle-diff: OK — every difference is recorded in docs/rust-rewrite/contract-inventory.md §9/§10\n'
}

# ── comparison helpers ───────────────────────────────────────────────────

exits_ok() { # exits_ok <name>
    local name=$1 oracle_status rust_status
    oracle_status=$(cat "$work/$name.elixir.status")
    rust_status=$(cat "$work/$name.rust.status")

    if [[ $oracle_status != 0 || $rust_status != 0 ]]; then
        fail "exit status: oracle $oracle_status, rust $rust_status ($(head -c 160 "$work/$name.rust.err" | tr '\n' ' '))"
        return 1
    fi

    return 0
}

json_ok() { # json_ok <file> <jq-expression> <what-is-wrong>
    if ! jq -e "$2" "$1" >/dev/null 2>&1; then
        fail "$3"
        return 1
    fi

    return 0
}

same() { diff -q "$1" "$2" >/dev/null 2>&1; }

first_difference() { # a one-line summary of a unified diff
    diff -u "$1" "$2" | sed -n '3,8p' | tr '\n' ' ' | tr -s ' '
}

# The number of consecutive newline bytes at the end of a file.
trailing_newlines() {
    local file=$1 size count=0
    size=$(wc -c <"$file" | tr -d ' ')

    while (( count < size )) &&
        [[ $(tail -c $((count + 1)) "$file" | wc -l | tr -d ' ') -eq $((count + 1)) ]]; do
        count=$((count + 1))
    done

    printf '%s' "$count"
}

# Syntax-checks the generated script with the shell it targets, when that shell
# is installed. Status 0: it parses, nothing printed. Status 1: it does not,
# the message is on stdout. Status 2: no check ran, the reason is on stdout.
parse_check() {
    local shell=$1 file=$2 output

    case $shell in
        bash | zsh | fish)
            if ! command -v "$shell" >/dev/null 2>&1; then
                printf '%s is not installed; parse check skipped' "$shell"
                return 2
            fi

            if ! output=$("$shell" -n "$file" 2>&1); then
                printf 'the %s script does not parse: %s' "$shell" "$(printf '%s' "$output" | tr '\n' ' ')"
                return 1
            fi
            ;;
        powershell)
            if ! command -v pwsh >/dev/null 2>&1; then
                printf 'pwsh is not installed; parse check skipped (the Linux CI job runs it)'
                return 2
            fi

            if ! output=$(pwsh -NoProfile -NonInteractive -Command \
                '$null = [scriptblock]::Create((Get-Content -Raw "'"$file"'"))' 2>&1); then
                printf 'the powershell script does not parse: %s' "$(printf '%s' "$output" | tr '\n' ' ')"
                return 1
            fi
            ;;
        *)
            printf 'no parse check for %s' "$shell"
            return 2
            ;;
    esac

    return 0
}

# ── Wave 1 reads and Wave 2 mutations: both binaries against one mock ────
#
# `--mock` runs every case twice — the frozen escript and the release binary —
# against one instance of the testkit's standalone mock
# (`cargo build -p ado-testkit --bin mock`; ADO_ORACLE_MOCK overrides the path,
# ADO_ORACLE_SCENARIO the route table). ADO_SERVER points at the mock, so neither
# binary reaches Azure, and ADO_ORG/ADO_PAT are synthetic. Unlike the Wave 0 cases,
# each case gets its own home, config directory and working directory per side:
# `login` writes a config and a credential, and `download` writes a file, so one
# case must not hand state to the next.
#
# For every case the harness compares
#
#   * the exit status;
#   * the requests the mock recorded — method, path, the query as parsed pairs
#     (D12: Rust's merged-then-caller order and the Elixir's key-sorted map are the
#     same pairs, so this compares semantics rather than bytes), the body as JSON
#     when it is one, and whether a route answered it;
#   * the JSON envelope on stdout (`jq -S`, D1);
#
# and prints MATCH / EXPECTED-DIFF / DIFF exactly as the Wave 0 cases do. Human
# output is compared only where it is the whole output (`pipelines-artifacts
# download`'s success line, the prompt cases), with the oracle's ANSI colour
# stripped first (§8, D11).
#
# A mutation case has four additions:
#
#   * **Bodies are contract, and a route can pin one.** A scenario route may carry
#     `request_body` (a string compares the bytes as sent; an object or array
#     compares structurally, so key order and whitespace cannot fail a body the
#     two sides send identically). The mock records `body_matched` on the request
#     line, so a body no route pins is still compared between the two sides — and a
#     body a route pins is checked against that pin even when both sides send the
#     same wrong bytes. Either mismatch fails the case.
#   * **A route can require query pairs.** A scenario route may carry `query` (an
#     object of pairs, compared as sent, with `*` matching any value). A route that
#     requires pairs is tried before one that does not, so two GETs on one path —
#     the attachment download's metadata and raw fetches — can be answered with
#     different bodies, and the case can discriminate the chain it claims to. The
#     `scenario coverage` case checks the requirement too, so a route whose pairs no
#     request ever carries cannot be dead.
#   * **stdin is scripted, never inherited.** Every run reads `case_stdin`; unset
#     means an empty file (EOF), so no case can block on a terminal or read the
#     developer's stdin. A prompt case sets it to the exact bytes to feed, e.g.
#     `case_stdin=$'y\n'`.
#   * **Prompts have their own stdout modes.** `stdout_mode=prompt-text` and
#     `prompt-json` strip the oracle's `[y/N]` prompt line from its stdout before
#     the normal text/envelope comparison and assert D31's pin: the oracle prompted
#     on stdout (the case is stale otherwise) and this build's prompt went to
#     stderr, leaving stdout to carry exactly one document under `--json` (`jq -e`
#     already rejects a second). Wording is not compared — the question text and
#     `Aborted.` are §8 surface; the exit status and the recorded requests carry
#     the refusal/proceed contract.
#
# A case may set `rest_rule`, `rest_norm`, `envelope_rule`, `status_rule`,
# `expect_statuses`, `stdout_mode`, `case_org`, `case_pat`, `case_extra`, `case_stdin`
# or `compare_files` immediately before it; `mock_case` clears them afterwards, so a
# rule cannot leak into the next case. A case that meets a difference no rule covers
# is a finding, not a row to invent.
#
# Two of those mechanisms make a case assert a *direction* rather than record an
# expected difference:
#
#   * `expect_statuses='<oracle> <rust>'` pins the exact pair of exit statuses. A
#     `status_rule` only fires when the two statuses differ, so a case whose point
#     is "ours refuses where the oracle does not" would read MATCH if this build
#     regressed to the oracle's shape. With the pair asserted, that regression
#     fails the case by name.
#   * `rest_norm` names the jq filter that mechanically expresses a `rest_rule`
#     (a query spelling, say). When it is set, the raw request lists may differ
#     only in that way: the filter is applied to both sides' projections and
#     anything still different — a missing or extra request included — fails the
#     case, where a bare `rest_rule` rules the whole request list away.

mock_bin=${ADO_ORACLE_MOCK:-$root/target/debug/mock}
mock_scenario=${ADO_ORACLE_SCENARIO:-$root/scripts/oracle-mock-scenario.json}
mock_url=
mock_log=
mock_requests=
mock_org=ado-harness
mock_pat=harness-pat

rest_rule=
rest_norm=
envelope_rule=
status_rule=
expect_statuses=
stdout_mode=json
case_org=$mock_org
case_pat=$mock_pat
case_extra=()
case_stdin=
compare_files=()

start_mock() {
    if [[ ! -x $mock_bin ]]; then
        printf 'oracle-diff: %s is missing; run `cargo build -p ado-testkit --bin mock` first\n' "$mock_bin" >&2
        exit 2
    fi

    if [[ ! -f $mock_scenario ]]; then
        printf 'oracle-diff: the scenario %s is missing\n' "$mock_scenario" >&2
        exit 2
    fi

    mock_log=$work/mock.log
    mock_requests=$work/mock-requests.jsonl
    : >"$mock_requests"

    "$mock_bin" --scenario "$mock_scenario" --record "$mock_requests" --port 0 >"$mock_log" 2>&1 &
    mock_pid=$!

    local waited=0
    while (( waited < 100 )); do
        mock_url=$(head -n1 "$mock_log" 2>/dev/null)
        [[ $mock_url =~ ^http://127\.0\.0\.1:[0-9]+$ ]] && return 0
        sleep 0.1
        waited=$((waited + 1))
    done

    printf 'oracle-diff: the mock did not report its origin; its log:\n' >&2
    cat "$mock_log" >&2
    exit 2
}

stop_mock() {
    if [[ -n $mock_pid ]]; then
        kill "$mock_pid" 2>/dev/null
        wait "$mock_pid" 2>/dev/null
        mock_pid=
    fi
}

mock_request_count() {
    if [[ -f $mock_requests ]]; then
        wc -l <"$mock_requests" | tr -d ' '
    else
        printf '0'
    fi
}

# One side of one case: a fresh home, config directory and working directory; the
# case's environment; the case's stdin; and the slice of the mock's log the run
# appended. Without `case_stdin` the run reads an empty file, so a prompt that no
# case scripted gets EOF rather than the terminal's input.
mock_run() { # mock_run <name> <side> <binary> <args...>
    local name=$1 side=$2 binary=$3
    shift 3
    local home=$work/homes/$name.$side cwd=$work/run/$name.$side before after stdin=/dev/null
    mkdir -p "$home" "$cwd"

    if [[ -n $case_stdin ]]; then
        stdin=$work/$name.$side.stdin
        printf '%s' "$case_stdin" >"$stdin"
    fi

    local -a environment=(
        -u ADO_ORG -u ADO_PAT -u ADO_SERVER
        "HOME=$home" "XDG_CONFIG_HOME=$home/config" "ADO_SERVER=$mock_url"
    )
    [[ -n $case_org ]] && environment+=("ADO_ORG=$case_org")
    [[ -n $case_pat ]] && environment+=("ADO_PAT=$case_pat")
    environment+=(${case_extra[@]+"${case_extra[@]}"})

    before=$(mock_request_count)
    (
        cd "$cwd" || exit 2
        env "${environment[@]}" "$binary" "$@" >"$work/$name.$side" 2>"$work/$name.$side.err" <"$stdin"
    )
    printf '%s' "$?" >"$work/$name.$side.status"

    after=$(mock_request_count)
    sed -n "$((before + 1)),$((after))p" "$mock_requests" >"$work/$name.$side.requests"
}

# One case: <slug> <label> <args...>.
mock_case() {
    local slug=$1 label=$2
    shift 2

    start_case "$label"
    mock_run "$slug" elixir "$elixir_bin" "$@"
    mock_run "$slug" rust "$rust_bin" "$@"
    mock_exit_check "$slug"
    mock_requests_check "$slug"
    mock_stdout_check "$slug"
    mock_files_check "$slug"

    rest_rule=
    rest_norm=
    envelope_rule=
    status_rule=
    expect_statuses=
    stdout_mode=json
    case_org=$mock_org
    case_pat=$mock_pat
    case_extra=()
    case_stdin=
    compare_files=()

    finish_case
}

mock_exit_check() { # the status is contract: 0 success, 1 every error (§6.3)
    local slug=$1 elixir_status rust_status
    elixir_status=$(cat "$work/$slug.elixir.status")
    rust_status=$(cat "$work/$slug.rust.status")

    # The asserted pair first: a case that names the direction of a ruled status
    # difference fails when the pair moves, even when the two sides agree again.
    if [[ -n $expect_statuses ]]; then
        local expected_elixir expected_rust
        read -r expected_elixir expected_rust <<<"$expect_statuses"

        if [[ $elixir_status != "$expected_elixir" || $rust_status != "$expected_rust" ]]; then
            fail "the expected statuses are oracle $expected_elixir, rust $expected_rust; got oracle $elixir_status, rust $rust_status"
        elif [[ $elixir_status == "$rust_status" ]]; then
            note "exit $rust_status on both"
        else
            ruled "${status_rule:-the expected status pair}: oracle $elixir_status, rust $rust_status"
        fi

        return
    fi

    if [[ $elixir_status == "$rust_status" ]]; then
        note "exit $rust_status on both"
    elif [[ -n $status_rule ]]; then
        ruled "$status_rule: oracle $elixir_status, rust $rust_status"
    else
        fail "exit status: oracle $elixir_status, rust $rust_status ($(head -c 160 "$work/$slug.rust.err" | tr '\n' ' '))"
    fi
}

# The query as parsed, decoded pairs: `+` is a space (both encoders write form
# encoding) and every pair is percent-decoded, so the Elixir's key-sorted map and
# Rust's merged order compare equal (§10, D12).
query_pairs_filter='[.query | split("&")[] | select(. != "")
    | (split("=")) as $pair
    | (($pair[0] | gsub("\\+"; "%20") | @urid) + "="
       + (($pair[1:] | join("=")) | gsub("\\+"; "%20") | @urid))] | sort'
requests_filter="map({
    method,
    path,
    query: $query_pairs_filter,
    body: (if .body == null then null else (.body | fromjson? // .) end),
    matched
})"

# D25's query difference, mechanically: the frozen `get_raw/2` glues
# `?api-version=7.1` onto a path that already carries `?fileName=…`, so its one
# pair's value swallows the version, where this build sends the two pairs. Splitting
# that pair reproduces this build's spelling; a case that sets
# `rest_norm=$d25_query_norm` still fails on any other request difference.
d25_query_norm='map(.query |= ([.[] | if startswith("fileName=") and (index("?api-version=") != null)
    then (. | split("?api-version=")) as $parts
       | ("fileName=" + ($parts[0][9:])), ("api-version=" + ($parts[1]))
    else . end] | sort))'

mock_requests_check() {
    local slug=$1 unmatched body_mismatch

    jq -s -c "$requests_filter" "$work/$slug.elixir.requests" >"$work/$slug.requests.elixir"
    jq -s -c "$requests_filter" "$work/$slug.rust.requests" >"$work/$slug.requests.rust"

    unmatched=$(
        cat "$work/$slug.elixir.requests" "$work/$slug.rust.requests" |
            jq -r 'select(.matched == false) | "\(.method) \(.path)"' | sort -u | tr '\n' ' '
    )
    unmatched=${unmatched% }

    if [[ -n $unmatched ]]; then
        fail "the mock has no route for: $unmatched"
        return
    fi

    # A route that declares `request_body` pins it: the mock answered the request
    # anyway and wrote its verdict to the log, so the failing check names the body
    # rather than routing the case away.
    body_mismatch=$(
        cat "$work/$slug.elixir.requests" "$work/$slug.rust.requests" |
            jq -r 'select(.body_matched == false) | "\(.method) \(.path) sent \(.body)"' | head -n1
    )

    if [[ -n $body_mismatch ]]; then
        fail "a request body does not match the route's request_body: $body_mismatch"
    fi

    if same "$work/$slug.requests.elixir" "$work/$slug.requests.rust"; then
        local requests
        requests=$(wc -l <"$work/$slug.elixir.requests" | tr -d ' ')

        if [[ $requests == 0 ]]; then
            note "no request on either side"
        else
            note "requests identical ($requests)"
        fi
        return
    fi

    # A rule with a mechanical form asserts more than it forgives: the normaliser
    # is applied to both sides and anything it does not account for fails the case,
    # so a rule can no longer absorb a difference it does not describe.
    if [[ -n $rest_norm ]]; then
        jq -c "$rest_norm" "$work/$slug.requests.elixir" >"$work/$slug.requests.norm.elixir"
        jq -c "$rest_norm" "$work/$slug.requests.rust" >"$work/$slug.requests.norm.rust"

        if same "$work/$slug.requests.norm.elixir" "$work/$slug.requests.norm.rust"; then
            ruled "$rest_rule"
        else
            fail "the recorded requests differ beyond the rule's normalisation: $(first_difference "$work/$slug.requests.norm.elixir" "$work/$slug.requests.norm.rust")"
        fi

        return
    fi

    if [[ -n $rest_rule ]]; then
        ruled "$rest_rule"
    else
        fail "the recorded requests differ: $(first_difference "$work/$slug.requests.elixir" "$work/$slug.requests.rust")"
    fi
}

# stdout with the ANSI colour the oracle wraps its lines in removed (§8, D11) and
# trailing blank lines dropped (its halt-success artefact).
strip_colour() {
    sed -E 's/\x1b\[[0-9;]*m//g; s/[[:space:]]+$//' "$1" |
        awk '{ if ($0 == "") { blanks++ } else { while (blanks > 0) { print ""; blanks-- } print } }'
}

# The prompt cases' stdout: the oracle writes its prompt to stdout (D31's captured
# half), so the prompt line sits in front of whatever the invocation printed. It is
# removed, colour and all, before the normal comparison; the candidate's prompt is
# on stderr, which the mode asserts separately.
prompt_strip() { # prompt_strip <file> <out>
    sed -E 's/\x1b\[[0-9;]*m//g' "$1" | grep -v '\[y/N\]' >"$2"
}

# A prompt case's two premises: the oracle really prompted on stdout (otherwise the
# case is stale and its stripping is meaningless), and D31 holds — this build's
# prompt went to stderr, so stdout carries no prompt and, under --json, exactly one
# document (the mode's `jq -e` check is what rejects a second).
mock_prompt_check() {
    local slug=$1

    if ! grep -q '\[y/N\]' "$work/$slug.elixir"; then
        fail "the oracle printed no [y/N] prompt on stdout: this prompt case is stale"
        return 1
    fi

    if [[ ! -s $work/$slug.rust.err ]]; then
        fail "the candidate's stderr is empty: the prompt did not go to stderr (D31)"
        return 1
    fi

    return 0
}

mock_stdout_check() {
    local slug=$1 elixir_out=$work/$slug.elixir

    case $stdout_mode in
        json)
            # The oracle's halt_success/1 artefact is a coloured empty line after
            # its document, even under --json; strip it (as the text modes already
            # do) so the document is what jq parses and compares.
            strip_colour "$work/$slug.elixir" >"$work/$slug.stdout.elixir"
            elixir_out=$work/$slug.stdout.elixir
            ;;
        text)
            strip_colour "$work/$slug.elixir" >"$work/$slug.stdout.elixir"
            ;;
        prompt-text | prompt-json)
            mock_prompt_check "$slug" || return
            prompt_strip "$work/$slug.elixir" "$work/$slug.stdout.elixir"
            strip_colour "$work/$slug.stdout.elixir" >"$work/$slug.stdout.elixir.plain"
            mv "$work/$slug.stdout.elixir.plain" "$work/$slug.stdout.elixir"
            elixir_out=$work/$slug.stdout.elixir
            ;;
    esac

    if [[ $stdout_mode == text || $stdout_mode == prompt-text ]]; then
        strip_colour "$work/$slug.rust" >"$work/$slug.stdout.rust"

        if same "$work/$slug.stdout.elixir" "$work/$slug.stdout.rust"; then
            note "stdout: '$(head -n1 "$work/$slug.stdout.rust" 2>/dev/null | head -c 60)'"
        elif [[ -n $envelope_rule ]]; then
            ruled "$envelope_rule"
        else
            fail "stdout differs: $(first_difference "$work/$slug.stdout.elixir" "$work/$slug.stdout.rust")"
        fi
        return
    fi

    if ! jq -e . "$work/$slug.rust" >/dev/null 2>&1; then
        fail "the Rust stdout is not a JSON envelope: $(head -c 120 "$work/$slug.rust" | tr '\n' ' ')"
        return
    fi

    if ! jq -e . "$elixir_out" >/dev/null 2>&1; then
        # The oracle's halt_error paths print their error on stderr and no envelope
        # at all, even under --json (D4).
        if [[ ! -s $elixir_out && -s $work/$slug.elixir.err ]]; then
            if [[ -n $envelope_rule ]]; then
                ruled "$envelope_rule"
            else
                fail "the oracle emitted no envelope and no stdout: $(head -c 120 "$work/$slug.elixir.err" | tr '\n' ' ')"
            fi
        elif [[ -n $envelope_rule ]]; then
            ruled "$envelope_rule"
        else
            fail "the oracle's stdout is not a JSON envelope: $(head -c 120 "$elixir_out" | tr '\n' ' ')"
        fi
        return
    fi

    jq -S . "$elixir_out" >"$work/$slug.envelope.elixir"
    jq -S . "$work/$slug.rust" >"$work/$slug.envelope.rust"

    if same "$work/$slug.envelope.elixir" "$work/$slug.envelope.rust"; then
        note "envelope identical ($(wc -c <"$work/$slug.envelope.rust" | tr -d ' ') bytes normalised)"
    elif [[ -n $envelope_rule ]]; then
        ruled "$envelope_rule"
    else
        fail "the envelopes differ: $(first_difference "$work/$slug.envelope.elixir" "$work/$slug.envelope.rust")"
    fi
}

# The bytes a download wrote — which no envelope carries — and the temp file the
# streamed write uses, which must not survive it (T9's round 2/3).
mock_files_check() {
    local slug=$1 file elixir_file rust_file leftovers

    for file in ${compare_files[@]+"${compare_files[@]}"}; do
        elixir_file=$work/run/$slug.elixir/$file
        rust_file=$work/run/$slug.rust/$file

        if [[ ! -f $elixir_file || ! -f $rust_file ]]; then
            fail "$file: oracle $([[ -f $elixir_file ]] && printf 'wrote it' || printf 'did not'), rust $([[ -f $rust_file ]] && printf 'wrote it' || printf 'did not')"
            continue
        fi

        if same "$elixir_file" "$rust_file"; then
            note "$file: $(wc -c <"$rust_file" | tr -d ' ') bytes identical"
        else
            fail "$file differs: $(cmp "$elixir_file" "$rust_file" 2>&1 | head -n1)"
        fi

        leftovers=$(find "$work/run/$slug.elixir" "$work/run/$slug.rust" -name '*.tmp' -print | tr '\n' ' ')
        leftovers=${leftovers% }

        if [[ -n $leftovers ]]; then
            fail "a temp file survived the download: $leftovers"
        else
            note "no .tmp sibling left behind"
        fi
    done
}

# The scenario's own coverage: every route it declares was requested at least once,
# so a route no case exercises cannot hide a missing case. A route's identity includes
# its query requirement — a query-requiring route is tried before a query-blind one,
# so a dead requirement would otherwise be invisible: its path is exercised by the
# neighbour.
mock_scenario_check() {
    local missing total

    start_case "scenario coverage"

    total=$(jq '[.responses[]] | length' "$mock_scenario")

    missing=$(jq -n -r --arg base "$mock_url" --slurpfile scenario "$mock_scenario" \
        --slurpfile requests "$mock_requests" '
        def sent_pairs($q): [ $q | split("&")[] | select(. != "")
            | (split("=")) as $p | {key: $p[0], value: ($p[1:] | join("="))} ];
        def carries($request; $required):
            all($required[];
                . as $want
                | (sent_pairs($request.query)
                   | any(. as $sent
                       | $sent.key == $want.key
                         and ($want.value == "*" or $sent.value == $want.value))));
        def route_label($route):
            "\($route.method) \($route.path)"
            + (($route.query // {} | to_entries | map("\(.key)=\(.value)") | sort) as $pairs
               | if ($pairs | length) == 0 then "" else " [\($pairs | join(","))]" end);

        [ $scenario[0].responses[]
          | . as $route
          | select(
              ([ $requests[]
                 | select((.method | ascii_downcase) == ($route.method | ascii_downcase)
                          and (.path | sub($base; "{base}")) == $route.path
                          and carries(.; ($route.query // {} | to_entries))) ]
               | length) == 0)
          | route_label($route) ]
        | join("; ")')

    if [[ -n $missing ]]; then
        fail "routes no case exercised: $missing"
    else
        note "every one of the $total scenario routes was exercised"
    fi

    finish_case
}

# Every Wave 1 command, in the order of the spec's §2 scope table. The ruled
# differences are the inventory's: D4 (the oracle's halt_error paths under --json),
# D11b (our config file is the new one), D12 (query pair order), D16 (blank values
# read as unset), D18 (the hyphenated invocation), D19 (projects list's parameter
# names), D20 (the repaired WIQL), D21 (an empty WIQL result under --json), D24 (the
# error body stays the upstream bytes), D25 (the absolute artifact downloadUrl) and
# D27c (logout's message).
run_mock_cases() {
    start_mock
    printf 'oracle-diff: --mock: %s serving %s\n' "$mock_url" "$mock_scenario"
    printf 'oracle-diff: oracle %s, candidate %s\n\n' "$elixir_bin" "$rust_bin"

    # `prs comments update --content @<file>` reads this: both binaries run from
    # their own temp cwd, so the path is absolute and shared.
    printf 'File body\n\n' >"$work/comments-body.md"

    # ── projects ──

    mock_case projects-list "projects list" projects list --json

    rest_rule='D19: the frozen CLI sends state/top/skip where this build sends the intended stateFilter/$top/$skip'
    mock_case projects-list-filters "projects list --state/--top/--skip" \
        projects list --state wellFormed --top 1 --skip 0 --json

    rest_rule='D19: present means sent — the empty state and the two zeros reach the wire on both sides, under the two spellings'
    mock_case projects-list-zeros "projects list --state '' --top 0 --skip 0" \
        projects list --state "" --top 0 --skip 0 --json

    mock_case projects-show "projects show" projects show Alpha --json
    mock_case projects-show-capabilities "projects show --capabilities" \
        projects show Alpha --capabilities --json

    # ── repositories ──

    mock_case repos-list "repos list" repos list Alpha --json

    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with inspect/2'
    mock_case repos-list-error "repos list (500)" repos list Broken --json

    mock_case repos-show "repos show" repos show Alpha Alpha.Core --json
    mock_case repos-branches "repos branches" repos branches Alpha Alpha.Core --json

    # ── work items ──

    mock_case workitems-list "workitems list" workitems list Alpha --json

    rest_rule='D20: the frozen CLI sends its malformed WIQL (a leading AND, doubled ANDs) where this build ships valid WIQL'
    mock_case workitems-list-filters "workitems list --type/--state/--assigned-to" \
        workitems list Alpha --type Bug --state Active --assigned-to alice --json

    envelope_rule='D21: an empty WIQL result under --json — the frozen CLI prints human text, this build the value envelope'
    mock_case workitems-list-empty "workitems list (empty result)" workitems list Empty --json

    mock_case workitems-show "workitems show" workitems show 42 --json
    mock_case workitems-query "workitems query" workitems query Alpha \
        --wiql "SELECT [System.Id] FROM WorkItems WHERE [System.State] = 'Active'" --json

    # ── pull requests ──

    mock_case prs-list "prs list" prs list Alpha Alpha.Core --json
    mock_case prs-show "prs show" prs show Alpha Alpha.Core 137 --json

    envelope_rule='D4: the frozen CLI writes the 404 to stderr with no envelope under --json where this build emits the error envelope (W1-R27)'
    mock_case prs-show-missing "prs show (404)" prs show Alpha Alpha.Core 999 --json

    # ── pipelines ──

    mock_case pipelines-list "pipelines list" pipelines list Alpha --json
    mock_case pipelines-show "pipelines show" pipelines show Alpha 12 --json

    envelope_rule='D4: the frozen CLI writes the 404 to stderr with no envelope under --json where this build emits the error envelope (W1-R27)'
    mock_case pipelines-show-missing "pipelines show (404)" pipelines show Alpha 999 --json

    # ── pipelines-builds ──

    mock_case builds-list "pipelines-builds list" pipelines-builds list Alpha --json
    mock_case builds-show "pipelines-builds show" pipelines-builds show Alpha 128 --json
    mock_case builds-tags "pipelines-builds tags list" pipelines-builds tags list Alpha 128 --json
    mock_case builds-definitions "pipelines-builds definitions list" \
        pipelines-builds definitions list Alpha --json

    envelope_rule='D18: the space spelling is a usage error on both sides; the wording is §8 regenerated surface (D5) and only the hyphenated spelling parses'
    stdout_mode=text
    mock_case builds-space-spelling "pipelines builds list (space spelling)" \
        pipelines builds list Alpha --json

    # ── pipelines-artifacts ──

    mock_case artifacts-list "pipelines-artifacts list" \
        pipelines-artifacts list Alpha 7 99 --json

    stdout_mode=text
    compare_files=(out.zip)
    mock_case artifacts-download "pipelines-artifacts download (relative downloadUrl)" \
        pipelines-artifacts download Alpha 8 99 drop --output out.zip --json

    rest_rule='D25: an absolute downloadUrl is requested verbatim with no added api-version, where the frozen client prepends its base and org-injects'
    stdout_mode=text
    compare_files=(out.zip)
    mock_case artifacts-download-absolute "pipelines-artifacts download (absolute downloadUrl)" \
        pipelines-artifacts download Alpha 7 99 drop --output out.zip --json

    # ── login and logout ──

    envelope_rule='D11b: the credential is saved to <config dir>/ado/config.toml, not the legacy ~/.ado_cli/config.json'
    mock_case login-pat "login --method pat" \
        login --method pat --org "$mock_org" --pat "$mock_pat" --json

    envelope_rule='D16: a blank value reads as unset here, so --pat "" is a validation error where the frozen CLI stores an empty token'
    status_rule='D16: --pat "" reads as unset here (exit 1) where the frozen CLI treats it as a value and exits 0'
    expect_statuses='0 1'
    case_pat=
    mock_case login-blank-pat "login --method pat --pat ''" \
        login --method pat --org "$mock_org" --pat "" --json

    envelope_rule='D16: a blank ADO_PAT leaves no method to infer, so this build refuses the invocation (the message wording is §8)'
    status_rule='D16: a blank ADO_PAT reads as unset here (exit 1) where the frozen CLI infers method=pat from it and exits 0'
    expect_statuses='0 1'
    case_pat=
    case_extra=("ADO_PAT=")
    mock_case login-blank-env-pat "login with a blank ADO_PAT" login --json

    envelope_rule='D27c: the message says what was removed instead of the legacy ~/.ado_cli/config.json path'
    mock_case logout "logout" logout --json

    envelope_rule='D27c: the message says what was removed instead of the legacy ~/.ado_cli/config.json path'
    mock_case logout-org "logout --org" logout --org "$mock_org" --json

    # ── Wave 2 mutations: projects and repos (Tasks 2–3) ──
    #
    # The five mutations' REST surface is compared field by field — method, path,
    # query and body — and each success route carries the `request_body` pin, so a
    # body that drifts on either side fails the case by name even when the two
    # sides send the same wrong bytes. Under `--json` the frozen write paths print
    # their human success line where this build emits the §6.1 value/message
    # envelope (D33), so every success case carries that envelope rule. The prompt
    # cases use the prompt modes: the frozen CLI's question is on stdout (D31) and
    # is stripped before the comparison, the confirmed path compares this build's
    # single-document stdout, the refusal path is empty stdout with exit 1 on both
    # sides (D32), and the EOF case carries D30's `status_rule` where the frozen
    # CLI exits 0.

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the value envelope'
    mock_case projects-create "projects create" projects create Created \
        --description "The created project" --visibility public --process agile \
        --source-control Tfvc --json

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the value envelope'
    mock_case projects-update "projects update" projects update Alpha \
        --name Renamed --description "Renamed project" --json

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the message envelope'
    mock_case projects-delete-force "projects delete --force" projects delete Alpha --force --json

    case_stdin=$'y\n'
    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the message envelope'
    stdout_mode=prompt-json
    mock_case projects-delete-confirmed "projects delete (confirmed)" projects delete Alpha --json

    case_stdin=$'n\n'
    stdout_mode=prompt-text
    mock_case projects-delete-refused "projects delete (refused)" projects delete Alpha

    case_stdin=$'n\n'
    stdout_mode=prompt-text
    mock_case projects-delete-refused-json "projects delete (refused, --json)" projects delete Alpha --json

    status_rule='D30: the frozen CLI exits 0 on an unanswered prompt; this build refuses with exit 1'
    expect_statuses='0 1'
    stdout_mode=prompt-text
    mock_case projects-delete-eof "projects delete (EOF)" projects delete Alpha

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the value envelope'
    mock_case repos-create "repos create" repos create Alpha NewRepo --default-branch trunk --json

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the message envelope'
    mock_case repos-delete-force "repos delete --force" repos delete Alpha Alpha.Core --force --json

    case_stdin=$'y\n'
    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the message envelope'
    stdout_mode=prompt-json
    mock_case repos-delete-confirmed "repos delete (confirmed)" repos delete Alpha Alpha.Core --json

    case_stdin=$'n\n'
    stdout_mode=prompt-text
    mock_case repos-delete-refused "repos delete (refused)" repos delete Alpha Alpha.Core

    case_stdin=$'n\n'
    stdout_mode=prompt-text
    mock_case repos-delete-refused-json "repos delete (refused, --json)" repos delete Alpha Alpha.Core --json

    status_rule='D30: the frozen CLI exits 0 on an unanswered prompt; this build refuses with exit 1'
    expect_statuses='0 1'
    stdout_mode=prompt-text
    mock_case repos-delete-eof "repos delete (EOF)" repos delete Alpha Alpha.Core

    # ── Wave 2 mutations: pipelines and variable groups (Task 4) ──
    #
    # The nine commands' REST surface is compared method/path/query/body against the
    # captures, and each write route carries the `request_body` pin. R5's mock-`n`
    # re-verification is the two delete cases: both are run with `n` on stdin
    # against the mock, which serves the GETs, and the recorded requests show the
    # oracle proceeding anyway — these commands do not prompt. Their unknown-flag
    # cases pin the other half: neither has a `--force`, so the flag is a usage
    # error on both sides. The required-option case (R4) is the oracle's silent
    # exit 0 against this build's loud usage error.

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the value envelope'
    mock_case pipelines-run "pipelines run" pipelines run Alpha 12 \
        --branch feature/foo --variables 'ENV=staging,DEBUG=true' --json

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the value envelope'
    mock_case pipelines-create "pipelines create" pipelines create Alpha \
        --name 'New CI' --repo Alpha.Core --path pipelines/new.yml \
        --folder MyTeam/Frontend --json

    status_rule='D5/D23 (R4): a required option the oracle never validates is a silent exit 0 there; this build makes it a loud usage error'
    expect_statuses='0 1'
    stdout_mode=text
    mock_case pipelines-create-no-name "pipelines create (no --name)" \
        pipelines create Alpha --repo Alpha.Core --path pipelines/new.yml --json

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the value envelope'
    mock_case pipelines-update "pipelines update" pipelines update Alpha 12 \
        --name 'Alpha CI (renamed)' --path pipelines/renamed.yml --json

    envelope_rule='D4: the frozen CLI writes the guard to stderr with no envelope under --json where this build emits the error envelope'
    mock_case pipelines-update-no-options "pipelines update (no options)" \
        pipelines update Alpha 12 --json

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the message envelope'
    mock_case pipelines-delete "pipelines delete" pipelines delete Alpha 12 --json

    case_stdin=$'n\n'
    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the message envelope'
    mock_case pipelines-delete-stdin-n "pipelines delete (stdin n — no prompt)" \
        pipelines delete Alpha 12 --json

    envelope_rule='D5: the oracle prints the command help on stdout before its usage error; this build writes clap’s message to stderr alone'
    stdout_mode=text
    mock_case pipelines-delete-force "pipelines delete --force (unknown flag)" \
        pipelines delete Alpha 12 --force

    mock_case pipelines-vars-list "pipelines vars list" pipelines vars list Alpha --top 10 --json
    mock_case pipelines-vars-show "pipelines vars show" pipelines vars show Alpha 5 --json

    envelope_rule='D4: the frozen CLI writes the 404 to stderr with no envelope under --json where this build emits the error envelope'
    mock_case pipelines-vars-show-missing "pipelines vars show (404)" \
        pipelines vars show Alpha 999 --json

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the value envelope'
    mock_case pipelines-vars-create "pipelines vars create" pipelines vars create Alpha \
        --name new-group --description 'A new group' \
        --variables 'DB_HOST=db.example.com,DB_PASS=hunter2' --secret DB_PASS --json

    status_rule='D5/D23 (R4): a required option the oracle never validates is a silent exit 0 there; this build makes it a loud usage error'
    expect_statuses='0 1'
    stdout_mode=text
    mock_case pipelines-vars-create-no-name "pipelines vars create (no --name)" \
        pipelines vars create Alpha --json

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the value envelope'
    mock_case pipelines-vars-update "pipelines vars update" pipelines vars update Alpha 5 \
        --name prod-secrets-renamed --description 'Updated description' \
        --variables 'DB_HOST=db2.example.com,API_KEY=secret-value' --secret API_KEY --json

    case_stdin=$'n\n'
    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the message envelope'
    mock_case pipelines-vars-delete "pipelines vars delete (stdin n — no prompt)" \
        pipelines vars delete Alpha 5 --json

    case_stdin=$'n\n'
    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the message envelope'
    mock_case pipelines-vars-delete-lookup-miss "pipelines vars delete (project not listed)" \
        pipelines vars delete Beta 5 --json

    envelope_rule='D4: the frozen CLI writes the 404 to stderr with no envelope under --json where this build emits the error envelope'
    mock_case pipelines-vars-delete-missing "pipelines vars delete (404)" \
        pipelines vars delete Alpha 999 --json

    # ── Wave 2: pipelines variables and secure files (Task 5) ──
    #
    # Both deletes were re-run with `n` on stdin against the mock, which serves
    # the GETs: `variables delete` sends both requests (no prompt, R5) and
    # `secure_files delete --force` sends the DELETE. `secure_files delete`
    # without `--force` is the message guard, not a prompt: the oracle prints it
    # on stdout and exits 0 having sent nothing, this build refuses on stderr
    # with exit 1 and sends nothing (R6, D32). The upload posts raw bytes, not
    # JSON; its route's `request_body` is the UTF-8 fixture's exact text — the
    # log's lossy decode is exact for it (the pin cannot prove bytes that are not
    # valid UTF-8, which is why the fixture is text). The three captured upload
    # failure paths — the 409 conflict, a failed replace DELETE and a failed
    # lookup GET — each get their own project-named route with the same fixture
    # pin; the display-spelling case is the counter-case to the underscore
    # spelling the other secure-files cases invoke (Task 4's builds-space-spelling
    # analogue).

    mock_case pipelines-variables-list "pipelines variables list" \
        pipelines variables list Alpha 13 --json

    mock_case pipelines-variables-list-none "pipelines variables list (no variables)" \
        pipelines variables list Alpha 7 --json

    envelope_rule='D4: the frozen CLI writes the 404 to stderr with no envelope under --json where this build emits the error envelope'
    mock_case pipelines-variables-list-404 "pipelines variables list (404)" \
        pipelines variables list Alpha 999 --json

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the message envelope'
    mock_case pipelines-variables-create "pipelines variables create" \
        pipelines variables create Alpha 13 --key ENV --value staging --json

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the message envelope'
    mock_case pipelines-variables-create-secret "pipelines variables create --secret" \
        pipelines variables create Alpha 16 --key API_KEY --value s3cret --secret --json

    status_rule='D5/D23 (R4): a required option the oracle never validates is a silent exit 0 there; this build makes it a loud usage error'
    expect_statuses='0 1'
    stdout_mode=text
    mock_case pipelines-variables-create-no-key "pipelines variables create (no --key)" \
        pipelines variables create Alpha 13 --json

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the message envelope'
    mock_case pipelines-variables-create-empty "pipelines variables create (no variables map)" \
        pipelines variables create Alpha 7 --key ENV --value staging --json

    case_stdin=$'n\n'
    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the message envelope'
    mock_case pipelines-variables-delete "pipelines variables delete (stdin n — no prompt)" \
        pipelines variables delete Alpha 14 --key DEBUG --json

    status_rule='D5/D23 (R4): a required option the oracle never validates is a silent exit 0 there; this build makes it a loud usage error'
    expect_statuses='0 1'
    stdout_mode=text
    mock_case pipelines-variables-delete-no-key "pipelines variables delete (no --key)" \
        pipelines variables delete Alpha 14 --json

    envelope_rule='D5: the oracle prints the command help on stdout before its usage error; this build writes clap’s message to stderr alone'
    stdout_mode=text
    mock_case pipelines-variables-delete-force "pipelines variables delete --force (unknown flag)" \
        pipelines variables delete Alpha 14 --key DEBUG --force

    envelope_rule='D4: the frozen CLI writes the 404 to stderr with no envelope under --json where this build emits the error envelope'
    mock_case pipelines-variables-delete-404 "pipelines variables delete (404)" \
        pipelines variables delete Alpha 999 --key DEBUG --json

    mock_case pipelines-secure-files-list "pipelines secure_files list" \
        pipelines secure_files list Alpha --top 10 --json

    mock_case pipelines-secure-files-list-empty "pipelines secure_files list (empty)" \
        pipelines secure_files list Beta --json

    mock_case pipelines-secure-files-show "pipelines secure_files show" \
        pipelines secure_files show Alpha f47ac10b-58cc-4372-a567-0e02b2c3d479 --json

    envelope_rule='D4: the frozen CLI writes the 404 to stderr with no envelope under --json where this build emits the error envelope'
    mock_case pipelines-secure-files-show-404 "pipelines secure_files show (404)" \
        pipelines secure_files show Alpha 00000000-0000-4000-8000-000000000999 --json

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the message envelope'
    mock_case pipelines-secure-files-upload "pipelines secure_files upload" \
        pipelines secure_files upload Alpha cert.pem \
        --file "$root/crates/ado-testkit/fixtures/secure_file_upload.pem" --json

    status_rule='D5/D23 (R4): a required option the oracle never validates is a silent exit 0 there; this build makes it a loud usage error'
    expect_statuses='0 1'
    stdout_mode=text
    mock_case pipelines-secure-files-upload-no-file "pipelines secure_files upload (no --file)" \
        pipelines secure_files upload Alpha cert.pem --json

    envelope_rule='D4: the frozen CLI writes the local error to stderr with no envelope under --json where this build emits the error envelope'
    mock_case pipelines-secure-files-upload-absent-file "pipelines secure_files upload (missing file)" \
        pipelines secure_files upload Alpha cert.pem \
        --file "$root/crates/ado-testkit/fixtures/absent.pem" --json

    envelope_rule='D4: the frozen CLI writes the 409 conflict to stderr with no envelope under --json where this build emits the error envelope'
    mock_case pipelines-secure-files-upload-conflict "pipelines secure_files upload (409 conflict)" \
        pipelines secure_files upload UploadConflict cert.pem \
        --file "$root/crates/ado-testkit/fixtures/secure_file_upload.pem" --json

    case_stdin=$'n\n'
    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the message envelope'
    mock_case pipelines-secure-files-upload-allow-exists "pipelines secure_files upload --allow-exists (stdin n — no prompt)" \
        pipelines secure_files upload Alpha prod-cert.pem \
        --file "$root/crates/ado-testkit/fixtures/secure_file_upload.pem" --allow-exists --json

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the message envelope'
    mock_case pipelines-secure-files-upload-allow-exists-miss "pipelines secure_files upload --allow-exists (lookup miss)" \
        pipelines secure_files upload Alpha new-cert.pem \
        --file "$root/crates/ado-testkit/fixtures/secure_file_upload.pem" --allow-exists --json

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the message envelope'
    mock_case pipelines-secure-files-upload-allow-exists-lookup-fails "pipelines secure_files upload --allow-exists (lookup 500)" \
        pipelines secure_files upload LookupBroken cert.pem \
        --file "$root/crates/ado-testkit/fixtures/secure_file_upload.pem" --allow-exists --json

    mock_case pipelines-secure-files-upload-allow-exists-delete-fails "pipelines secure_files upload --allow-exists (delete failure)" \
        pipelines secure_files upload ReplaceBroken prod-cert.pem \
        --file "$root/crates/ado-testkit/fixtures/secure_file_upload.pem" --allow-exists --json

    case_stdin=$'n\n'
    status_rule='D32: the oracle prints its guard on stdout and exits 0 having sent nothing; this build refuses with exit 1 and sends nothing'
    expect_statuses='0 1'
    envelope_rule='D32: the oracle prints its guard on stdout and exits 0; this build writes the refusal to stderr with no document'
    stdout_mode=text
    mock_case pipelines-secure-files-delete-guard "pipelines secure_files delete (stdin n — no prompt, no --force)" \
        pipelines secure_files delete Alpha f47ac10b-58cc-4372-a567-0e02b2c3d479 --json

    case_stdin=$'n\n'
    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the message envelope'
    mock_case pipelines-secure-files-delete-force "pipelines secure_files delete --force (stdin n — no prompt)" \
        pipelines secure_files delete Alpha f47ac10b-58cc-4372-a567-0e02b2c3d479 --force --json

    envelope_rule='D4: the frozen CLI writes the 404 to stderr with no envelope under --json where this build emits the error envelope'
    mock_case pipelines-secure-files-delete-404 "pipelines secure_files delete (404)" \
        pipelines secure_files delete Alpha 00000000-0000-4000-8000-000000000999 --force --json

    envelope_rule='D18/R3: only the underscore spelling parses; the hyphenated display spelling is a usage error on both sides and the wording is §8 regenerated surface (D5)'
    stdout_mode=text
    mock_case pipelines-secure-files-display-spelling "pipelines secure-files list (display spelling)" \
        pipelines secure-files list Alpha --json

    # ── Wave 2: pipeline folders and the builds gaps (Task 6) ──
    #
    # R5: `pipelines-folders delete` was re-run against the mock with `n` on
    # stdin and on EOF and sent the DELETE both times — no prompt (the capture
    # review's blind spots are why every destructive command repeats the probe).
    # The oracle prints its human table or line under `--json` on every folder
    # command and on `queue`, `cancel` and `tags add`; this build emits the
    # value/message envelope (D21's class for the read, D33 for the writes). A
    # folder path's `/` separators stay separators here exactly as through the
    # oracle's `URI.encode/1`, so the spaced-path case's request bytes match —
    # only bytes like `?` are tightened (D22, captured). `--path`, `--definition`
    # and `--tags` are R4/D34's silent exit 0 there and a usage error here, and
    # none of the three has a `--force`.

    envelope_rule='D21: the frozen CLI prints its human table even under --json on this read; this build emits the value envelope'
    mock_case pipelines-folders-list "pipelines-folders list" \
        pipelines-folders list Folders --json

    envelope_rule='D21: the frozen CLI prints its human table even under --json on this read; this build emits the value envelope'
    mock_case pipelines-folders-list-path "pipelines-folders list --path" \
        pipelines-folders list Folders --path MyTeam/Frontend --json

    envelope_rule='D21: the frozen CLI prints "No folders found." even under --json; this build emits the empty value envelope'
    mock_case pipelines-folders-list-empty "pipelines-folders list (empty)" \
        pipelines-folders list FoldersEmpty --json

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the value envelope'
    mock_case pipelines-folders-create "pipelines-folders create" \
        pipelines-folders create Folders --path MyTeam/Frontend --json

    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with inspect/2'
    mock_case pipelines-folders-create-conflict "pipelines-folders create (409)" \
        pipelines-folders create FoldersBroken --path MyTeam/Frontend --json

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the message envelope'
    mock_case pipelines-folders-delete "pipelines-folders delete" \
        pipelines-folders delete Folders --path MyTeam/Frontend --json

    case_stdin=$'n\n'
    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the message envelope'
    mock_case pipelines-folders-delete-stdin-n "pipelines-folders delete (stdin n — no prompt)" \
        pipelines-folders delete Folders --path MyTeam/Frontend --json

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the message envelope'
    mock_case pipelines-folders-delete-spaced "pipelines-folders delete (spaced path)" \
        pipelines-folders delete FoldersSpaced --path 'My Team/Front end' --json

    mock_case pipelines-folders-delete-404 "pipelines-folders delete (404)" \
        pipelines-folders delete FoldersMissing --path MyTeam/Frontend --json

    status_rule='D5/D23 (R4): a required option the oracle never validates is a silent exit 0 there; this build makes it a loud usage error'
    expect_statuses='0 1'
    stdout_mode=text
    mock_case pipelines-folders-delete-no-path "pipelines-folders delete (no --path)" \
        pipelines-folders delete Folders --json

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the value envelope'
    mock_case pipelines-builds-queue "pipelines-builds queue" \
        pipelines-builds queue Builds --definition 5 --json

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the value envelope'
    mock_case pipelines-builds-queue-branch "pipelines-builds queue --branch" \
        pipelines-builds queue BuildsBranch --definition 7 --branch feature/foo --json

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the value envelope'
    mock_case pipelines-builds-queue-nolinks "pipelines-builds queue (no _links)" \
        pipelines-builds queue BuildsNoLinks --definition 5 --json

    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with inspect/2'
    mock_case pipelines-builds-queue-400 "pipelines-builds queue (400)" \
        pipelines-builds queue BuildsBroken --definition 5 --json

    status_rule='D5/D23 (R4): a required option the oracle never validates is a silent exit 0 there; this build makes it a loud usage error'
    expect_statuses='0 1'
    stdout_mode=text
    mock_case pipelines-builds-queue-no-definition "pipelines-builds queue (no --definition)" \
        pipelines-builds queue Builds --json

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the value envelope'
    mock_case pipelines-builds-cancel "pipelines-builds cancel" \
        pipelines-builds cancel Builds 128 --json

    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with inspect/2'
    mock_case pipelines-builds-cancel-404 "pipelines-builds cancel (404)" \
        pipelines-builds cancel BuildsMissing 128 --json

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the value envelope'
    mock_case pipelines-builds-tags-add "pipelines-builds tags add" \
        pipelines-builds tags add Builds 128 --tags 'release,prod,v1.2.3' --json

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the value envelope'
    mock_case pipelines-builds-tags-add-trim "pipelines-builds tags add (trim)" \
        pipelines-builds tags add BuildsTrim 128 --tags 'trimmed, spaced ' --json

    status_rule='D5/D23 (R4): a required option the oracle never validates is a silent exit 0 there; this build makes it a loud usage error'
    expect_statuses='0 1'
    stdout_mode=text
    mock_case pipelines-builds-tags-add-no-tags "pipelines-builds tags add (no --tags)" \
        pipelines-builds tags add Builds 128 --json

    # ── Wave 2: work item mutations (Task 7) ──
    #
    # Both writes are JSON-patch arrays under `application/json-patch+json`, and
    # every route pins the captured body, so a plain object, the wrong op or the
    # wrong field order fails the case by name even when both binaries send it.
    # `create`'s order is title, description, assigned-to, state, priority, tags;
    # `update`'s is tags (replace, first), title, description, state,
    # assigned-to, priority — the two orders are captured separately because they
    # differ. R5: `workitems delete` was re-run against the mock with `n` on
    # stdin and on EOF and sent its DELETE both times — no prompt — and the tree
    # has no `--force` to port (the unknown-flag case pins that). The
    # no-`--type`/`--title`/no-field guards are loud on both sides (exit 1, no
    # request); `update`'s is a `halt_error` on the oracle where this build emits
    # the error envelope (D4).

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the value envelope'
    mock_case workitems-create "workitems create" workitems create Alpha \
        --type Bug --title 'Checkout fails on expired cards' --json

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the value envelope'
    mock_case workitems-create-full "workitems create (all options)" workitems create Alpha \
        --type 'User Story' --title 'Payment retries for soft declines' \
        --description 'Description body' --assigned-to alice --state Active \
        --priority 2 --tags 'frontend,ui' --json

    stdout_mode=text
    mock_case workitems-create-no-type "workitems create (no --type)" \
        workitems create Alpha --json

    stdout_mode=text
    mock_case workitems-create-no-title "workitems create (no --title)" \
        workitems create Alpha --type Bug --json

    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with inspect/2'
    mock_case workitems-create-400 "workitems create (400)" \
        workitems create Broken --type Bug --title T --json

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the value envelope'
    mock_case workitems-update "workitems update" workitems update 42 \
        --title 'Checkout fails on expired cards (renamed)' --json

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the value envelope'
    mock_case workitems-update-all "workitems update (all options)" workitems update 43 \
        --title 'Payment retries for soft declines (renamed)' --description 'New body' \
        --state Closed --assigned-to bob --priority 1 --tags 'a,b' --json

    envelope_rule='D4: the frozen CLI writes the guard to stderr with no envelope under --json where this build emits the error envelope'
    mock_case workitems-update-no-options "workitems update (no options)" \
        workitems update 42 --json

    envelope_rule='D4: the frozen CLI writes the 404 to stderr with no envelope under --json where this build emits the error envelope'
    mock_case workitems-update-404 "workitems update (404)" \
        workitems update 999 --title T --json

    case_stdin=$'n\n'
    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the message envelope'
    mock_case workitems-delete-stdin-n "workitems delete (stdin n — no prompt)" \
        workitems delete 42 --json

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the message envelope'
    mock_case workitems-delete-eof "workitems delete (EOF — no prompt)" \
        workitems delete 42 --json

    envelope_rule='D4: the frozen CLI writes the 404 to stderr with no envelope under --json where this build emits the error envelope'
    mock_case workitems-delete-404 "workitems delete (404)" \
        workitems delete 999 --json

    envelope_rule='D5: the oracle prints the command help on stdout before its usage error; this build writes clap’s message to stderr alone'
    stdout_mode=text
    mock_case workitems-delete-no-id "workitems delete (no id)" \
        workitems delete --json

    envelope_rule='D5: the oracle prints the command help on stdout before its usage error; this build writes clap’s message to stderr alone'
    stdout_mode=text
    mock_case workitems-delete-force "workitems delete --force (unknown flag)" \
        workitems delete 42 --force

    # ── Wave 2: work item comments and attachments (Task 8) ──
    #
    # The two path spellings are captured, not assumed: the comments read uses
    # `workItems` (capital I), every other path `workitems`. Both comment writes
    # are one-operation `System.History` JSON patches whose **bodies** are pinned
    # byte-for-byte by their routes; the `application/json-patch+json` content type
    # is pinned by the integration tests and the captured request bytes, not by the
    # routes — the request log carries no headers. None of
    # the five commands prompts: the two writes and the download were run against
    # the mock with `n` on stdin (and on EOF in the captures), and the requests
    # went out. A missing `--text` is D34's class — the oracle's schema marks it
    # required and CliMate never enforces it, so it writes nothing and exits 0,
    # where this build is a loud usage error. The download's metadata GET and
    # raw-content GET share a path (the module asks for both under
    # `/_apis/wit/attachments/{id}`): the metadata route is query-blind and the raw
    # route requires the `fileName` pair, so the two GETs get different bodies — the
    # raw one the captured bytes — and a candidate that drops either GET fails on
    # the request count or on the file's bytes. The raw GET's `fileName` query is
    # this wave's second **D25** difference: the frozen `get_raw/2` glues
    # `?api-version=7.1` onto a path that already carries `?fileName=…` (one query
    # pair whose value swallows the version), and this build sends the two pairs
    # properly — so those cases set `rest_norm=$d25_query_norm`, which rules exactly
    # that spelling difference and nothing else. The success line
    # is the whole stdout on both sides, so the download cases are `text`-mode
    # with the module's line compared; the oracle appends its
    # `halt_success("Done.")` marker, which this build does not print (§8
    # regenerated surface), and that lone difference is the case's rule. A 302 is
    # refused with its true status on both sides (D8), the raw GET's status is
    # D25's classification, and the missing-positional, non-numeric-id and
    # unknown-flag rows are D5's usage error (captured: `download-no-id`,
    # `comments-list-underscore-id` and `comments-unknown-flag`; the same
    # `attachments-unknown-flag` capture is the attachments leaf's shape).

    mock_case workitems-comments-list "workitems comments list" \
        workitems comments list 42 --json

    stdout_mode=text
    mock_case workitems-comments-list-human "workitems comments list (human)" \
        workitems comments list 42

    mock_case workitems-comments-list-empty-array "workitems comments list (empty array)" \
        workitems comments list 8 --json

    envelope_rule='D21: the frozen CLI prints "No comments found." even under --json on a body without the comments key; this build emits the empty value envelope'
    mock_case workitems-comments-list-missing-key "workitems comments list (no comments key)" \
        workitems comments list 7 --json

    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with inspect/2'
    mock_case workitems-comments-list-404 "workitems comments list (404)" \
        workitems comments list 999 --json

    envelope_rule='D5: the oracle prints the command help on stdout before its usage error; this build writes clap’s message to stderr alone'
    stdout_mode=text
    mock_case workitems-comments-list-no-id "workitems comments list (no id)" \
        workitems comments list --json

    envelope_rule='D5: the oracle prints the command help on stdout before its usage error; this build writes clap’s message to stderr alone (captured: comments-unknown-flag)'
    stdout_mode=text
    mock_case workitems-comments-unknown-flag "workitems comments list --force (unknown flag)" \
        workitems comments list 42 --force

    envelope_rule='D5: the oracle prints the command help on stdout before its usage error; this build writes clap’s message to stderr alone (captured: comments-list-underscore-id)'
    stdout_mode=text
    mock_case workitems-comments-list-non-numeric-id "workitems comments list id (non-numeric id)" \
        workitems comments list id

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the message envelope'
    mock_case workitems-comments-add "workitems comments add" \
        workitems comments add 500 --text 'Looks good' --json

    case_stdin=$'n\n'
    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the message envelope'
    mock_case workitems-comments-add-stdin-n "workitems comments add (stdin n — no prompt)" \
        workitems comments add 500 --text 'Looks good' --json

    status_rule='D5/D23 (D34): a required option the oracle never validates is a silent exit 0 there; this build makes it a loud usage error'
    expect_statuses='0 1'
    stdout_mode=text
    mock_case workitems-comments-add-no-text "workitems comments add (no --text)" \
        workitems comments add 500 --json

    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with inspect/2'
    mock_case workitems-comments-add-404 "workitems comments add (404)" \
        workitems comments add 896 --text 'Looks good' --json

    envelope_rule='D5: the oracle prints the command help on stdout before its usage error; this build writes clap’s message to stderr alone'
    stdout_mode=text
    mock_case workitems-comments-add-no-id "workitems comments add (no id)" \
        workitems comments add --text 'Looks good'

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the message envelope'
    mock_case workitems-comments-update "workitems comments update" \
        workitems comments update 501 7 --text 'Edited text' --json

    case_stdin=$'n\n'
    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the message envelope'
    mock_case workitems-comments-update-stdin-n "workitems comments update (stdin n — no prompt)" \
        workitems comments update 501 7 --text 'Edited text' --json

    status_rule='D5/D23 (D34): a required option the oracle never validates is a silent exit 0 there; this build makes it a loud usage error'
    expect_statuses='0 1'
    stdout_mode=text
    mock_case workitems-comments-update-no-text "workitems comments update (no --text)" \
        workitems comments update 501 7 --json

    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with inspect/2'
    mock_case workitems-comments-update-404 "workitems comments update (404)" \
        workitems comments update 897 7 --text 'Edited text' --json

    envelope_rule='D5: the oracle prints the command help on stdout before its usage error; this build writes clap’s message to stderr alone'
    stdout_mode=text
    mock_case workitems-comments-update-no-comment-id "workitems comments update (no comment_id)" \
        workitems comments update 501 --text 'Edited text'

    mock_case workitems-attachments-list "workitems attachments list" \
        workitems attachments list 42 --json

    stdout_mode=text
    mock_case workitems-attachments-list-human "workitems attachments list (human)" \
        workitems attachments list 42

    mock_case workitems-attachments-list-empty-array "workitems attachments list (empty array)" \
        workitems attachments list 8 --json

    envelope_rule='D21: the frozen CLI prints "No attachments found." even under --json on a body without the attachments key; this build emits the empty value envelope'
    mock_case workitems-attachments-list-missing-key "workitems attachments list (no attachments key)" \
        workitems attachments list 7 --json

    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with inspect/2'
    mock_case workitems-attachments-list-404 "workitems attachments list (404)" \
        workitems attachments list 999 --json

    envelope_rule='D5: the oracle prints the command help on stdout before its usage error; this build writes clap’s message to stderr alone'
    stdout_mode=text
    mock_case workitems-attachments-list-no-id "workitems attachments list (no id)" \
        workitems attachments list --json

    rest_rule='D25: the frozen get_raw appends api-version onto a path that already carries ?fileName=… (one query pair whose value swallows the version); this build sends fileName and api-version as separate pairs'
    rest_norm=$d25_query_norm
    envelope_rule='§8: the frozen download prints its success line plus the halt_success "Done." marker; this build prints the line alone'
    stdout_mode=text
    compare_files=(out.bin)
    mock_case workitems-attachments-download-output "workitems attachments download" \
        workitems attachments download 42 att-1 --output out.bin

    rest_rule='D25: the frozen get_raw appends api-version onto a path that already carries ?fileName=… (one query pair whose value swallows the version); this build sends fileName and api-version as separate pairs'
    rest_norm=$d25_query_norm
    envelope_rule='§8: the frozen download prints its success line plus the halt_success "Done." marker; this build prints the line alone'
    stdout_mode=text
    compare_files=(out.bin)
    mock_case workitems-attachments-download-output-json "workitems attachments download (--json)" \
        workitems attachments download 42 att-1 --output out.bin --json

    rest_rule='D25: the frozen get_raw appends api-version onto a path that already carries ?fileName=… (one query pair whose value swallows the version); this build sends fileName and api-version as separate pairs'
    rest_norm=$d25_query_norm
    envelope_rule='§8: the frozen download prints its success line plus the halt_success "Done." marker; this build prints the line alone'
    stdout_mode=text
    compare_files=(notes.txt)
    mock_case workitems-attachments-download-default "workitems attachments download (default name)" \
        workitems attachments download 42 att-2

    rest_rule='D25: the frozen get_raw appends api-version onto a path that already carries ?fileName=… (one query pair whose value swallows the version); this build sends fileName and api-version as separate pairs'
    rest_norm=$d25_query_norm
    envelope_rule='§8: the frozen download prints its success line plus the halt_success "Done." marker; this build prints the line alone'
    stdout_mode=text
    compare_files=(attachment_att-3)
    mock_case workitems-attachments-download-no-name "workitems attachments download (no attributes.name)" \
        workitems attachments download 42 att-3

    case_stdin=$'n\n'
    rest_rule='D25: the frozen get_raw appends api-version onto a path that already carries ?fileName=… (one query pair whose value swallows the version); this build sends fileName and api-version as separate pairs'
    rest_norm=$d25_query_norm
    envelope_rule='§8: the frozen download prints its success line plus the halt_success "Done." marker; this build prints the line alone'
    stdout_mode=text
    compare_files=(out.bin)
    mock_case workitems-attachments-download-stdin-n "workitems attachments download (stdin n — no prompt)" \
        workitems attachments download 42 att-1 --output out.bin

    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with inspect/2'
    mock_case workitems-attachments-download-404 "workitems attachments download (404)" \
        workitems attachments download 42 att-404 --json

    envelope_rule='D8: the redirect is refused with its true status on both sides; the oracle’s message is the sign-in wording and this build names the missing Location header'
    mock_case workitems-attachments-download-redirect "workitems attachments download (302)" \
        workitems attachments download 42 att-redirect --json

    envelope_rule='D5: the oracle prints the command help on stdout before its usage error; this build writes clap’s message to stderr alone'
    stdout_mode=text
    mock_case workitems-attachments-download-no-attachment-id "workitems attachments download (no attachment_id)" \
        workitems attachments download 42 --json

    envelope_rule='D5: the oracle prints the command help on stdout before its usage error; this build writes clap’s message to stderr alone (captured: download-no-id)'
    stdout_mode=text
    mock_case workitems-attachments-download-no-id "workitems attachments download (no positionals)" \
        workitems attachments download

    envelope_rule='D5: the oracle prints the command help on stdout before its usage error; this build writes clap’s message to stderr alone (the same parser shape as the captured comments-list-underscore-id)'
    stdout_mode=text
    mock_case workitems-attachments-download-non-numeric-id "workitems attachments download (non-numeric id)" \
        workitems attachments download abc att-1

    # ── Wave 2: the pull request lifecycle mutations (Task 9) ──
    #
    # The five commands' captured shapes: `create` one POST (the four keys always,
    # `description` only when given), `complete` a GET for
    # `lastMergeSourceCommit.commitId` then a PATCH whose `mergeStrategy` key is
    # absent when the option is absent and maps squash→squashMerge,
    # rebase→rebaseMerge (an unknown value passes through), `abandon` one PATCH,
    # and `approve`/`vote` a `GET /_apis/connectionData` **with no api-version at
    # all** then a `PUT …/reviewers/{authenticatedUser.id}`. Every write route pins
    # the captured body, and each create case uses its own project (the collection
    # path is otherwise identical) so the pins cannot be shared by accident; the
    # five vote values and the approve case use per-id reviewer routes for the same
    # reason. R5: none of the five prompts — every one was re-run against the mock
    # with `n` on stdin and, for the state changes, on EOF, and the requests went
    # out; the stdin cases below are that evidence. The oracle's `create` without
    # `--description` (or `--title`/`--source`/`--target`) dies on the missing map
    # key and exits 0 silently, sending nothing — D35 for the absent optional
    # option (this build sends the body without the key), D34 for the required ones
    # (this build's clap is loud) — and `vote` without `--vote` is D34's row too.
    # The table's `--delete_source` is the schema's name, not an invocation (D17):
    # the frozen parser rejects it, as the unknown-flag case shows. The four write
    # commands' success lines are D33's envelope here; their error paths are D4/D24.

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the value envelope'
    mock_case prs-create "prs create" prs create Create Alpha.Core \
        --title 'Add checkout retries' --description 'Retries soft declines.' \
        --source feature/payments --target main --draft --json

    case_stdin=$'n\n'
    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the value envelope'
    mock_case prs-create-stdin-n "prs create (stdin n — no prompt)" prs create Create Alpha.Core \
        --title 'Add checkout retries' --description 'Retries soft declines.' \
        --source feature/payments --target main --draft --json

    rest_rule='D35: the oracle dies on the absent --description before any request (exit 0, no output); this build sends the body without the key'
    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the value envelope'
    mock_case prs-create-minimal "prs create (no --description)" prs create CreateMin Alpha.Core \
        --title 'Add checkout retries' --source feature/payments --target main --json

    status_rule='D34: a missing required option is a silent exit 0 in the oracle; this build is a loud usage error'
    expect_statuses='0 1'
    stdout_mode=text
    mock_case prs-create-no-title "prs create (no --title)" prs create CreateMin Alpha.Core \
        --source feature/payments --target main --json

    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with inspect/2'
    mock_case prs-create-400 "prs create (400)" prs create Broken Alpha.Core \
        --title T --description D --source s --target t --json

    envelope_rule='D5: the oracle prints the command help on stdout before its usage error; this build writes clap’s message to stderr alone'
    stdout_mode=text
    mock_case prs-create-unknown-flag "prs create --force (unknown flag)" prs create Create Alpha.Core \
        --title T --description D --source s --target t --force

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the value envelope'
    mock_case prs-complete "prs complete" prs complete Alpha Alpha.Core 137 --json

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the value envelope'
    mock_case prs-complete-options "prs complete (--delete-source --merge-strategy squash)" \
        prs complete Alpha Alpha.Core 151 --delete-source --merge-strategy squash --json

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the value envelope'
    mock_case prs-complete-empty-strategy "prs complete (--merge-strategy '')" \
        prs complete Alpha Alpha.Core 152 --merge-strategy '' --json

    case_stdin=$'n\n'
    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the value envelope'
    mock_case prs-complete-stdin-n "prs complete (stdin n — no prompt)" \
        prs complete Alpha Alpha.Core 137 --json

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the value envelope'
    mock_case prs-complete-eof "prs complete (EOF — no prompt)" \
        prs complete Alpha Alpha.Core 137 --json

    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with inspect/2'
    mock_case prs-complete-get-404 "prs complete (GET 404)" prs complete Alpha Alpha.Core 999 --json

    envelope_rule='D4: the frozen CLI writes the guard to stderr with no envelope under --json where this build emits the error envelope'
    mock_case prs-complete-no-commit "prs complete (no lastMergeSourceCommit)" \
        prs complete Alpha Alpha.Core 140 --json

    envelope_rule='D4: the frozen CLI writes the 404 to stderr with no envelope under --json where this build emits the error envelope'
    mock_case prs-complete-patch-404 "prs complete (PATCH 404)" prs complete Alpha Alpha.Core 141 --json

    envelope_rule='D4: the frozen CLI writes the 409 to stderr with no envelope under --json where this build emits the error envelope'
    mock_case prs-complete-patch-409 "prs complete (PATCH 409)" prs complete Alpha Alpha.Core 142 --json

    envelope_rule='D5: the oracle prints the command help on stdout before its usage error; this build writes clap’s message to stderr alone'
    stdout_mode=text
    mock_case prs-complete-underscore "prs complete --delete_source (unknown flag)" \
        prs complete Alpha Alpha.Core 137 --delete_source

    envelope_rule='D5: the oracle prints the command help on stdout before its usage error; this build writes clap’s message to stderr alone'
    stdout_mode=text
    mock_case prs-complete-no-id "prs complete (no pr_id)" prs complete Alpha Alpha.Core

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the value envelope'
    mock_case prs-abandon "prs abandon" prs abandon Alpha Alpha.Core 138 --json

    case_stdin=$'n\n'
    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the value envelope'
    mock_case prs-abandon-stdin-n "prs abandon (stdin n — no prompt)" \
        prs abandon Alpha Alpha.Core 138 --json

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the value envelope'
    mock_case prs-abandon-eof "prs abandon (EOF — no prompt)" prs abandon Alpha Alpha.Core 138 --json

    envelope_rule='D4: the frozen CLI writes the 404 to stderr with no envelope under --json where this build emits the error envelope'
    mock_case prs-abandon-404 "prs abandon (404)" prs abandon Alpha Alpha.Core 999 --json

    envelope_rule='D4: the frozen CLI writes the 409 to stderr with no envelope under --json where this build emits the error envelope'
    mock_case prs-abandon-409 "prs abandon (409)" prs abandon Alpha Alpha.Core 144 --json

    envelope_rule='D5: the oracle prints the command help on stdout before its usage error; this build writes clap’s message to stderr alone'
    stdout_mode=text
    mock_case prs-abandon-no-id "prs abandon (no pr_id)" prs abandon Alpha Alpha.Core

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the value envelope'
    mock_case prs-approve "prs approve" prs approve Alpha Alpha.Core 137 --json

    case_stdin=$'n\n'
    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the value envelope'
    mock_case prs-approve-stdin-n "prs approve (stdin n — no prompt)" \
        prs approve Alpha Alpha.Core 137 --json

    envelope_rule='D4: the frozen CLI writes the 404 to stderr with no envelope under --json where this build emits the error envelope'
    mock_case prs-approve-put-404 "prs approve (PUT 404)" prs approve Alpha Alpha.Core 999 --json

    envelope_rule='D4: the frozen CLI writes the 400 to stderr with no envelope under --json where this build emits the error envelope'
    mock_case prs-approve-put-400 "prs approve (PUT 400)" prs approve Alpha Alpha.Core 143 --json

    case_org=conn-broken
    envelope_rule='D4: the frozen CLI writes the guard to stderr with no envelope under --json where this build emits the error envelope'
    mock_case prs-approve-conn-broken "prs approve (connectionData 404)" \
        prs approve Alpha Alpha.Core 137 --json

    case_org=conn-no-id
    envelope_rule='D4: the frozen CLI writes the guard to stderr with no envelope under --json where this build emits the error envelope'
    mock_case prs-approve-conn-no-id "prs approve (connectionData without an id)" \
        prs approve Alpha Alpha.Core 137 --json

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the value envelope'
    mock_case prs-vote-10 "prs vote --vote 10" prs vote Alpha Alpha.Core 137 --vote 10 --json

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the value envelope'
    mock_case prs-vote-5 "prs vote --vote 5" prs vote Alpha Alpha.Core 145 --vote 5 --json

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the value envelope'
    mock_case prs-vote-0 "prs vote --vote 0" prs vote Alpha Alpha.Core 146 --vote 0 --json

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the value envelope'
    mock_case prs-vote-minus5 "prs vote --vote -5" prs vote Alpha Alpha.Core 147 --vote -5 --json

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the value envelope'
    mock_case prs-vote-minus10 "prs vote --vote -10" prs vote Alpha Alpha.Core 148 --vote -10 --json

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the value envelope'
    mock_case prs-vote-unknown "prs vote --vote 7" prs vote Alpha Alpha.Core 149 --vote 7 --json

    case_stdin=$'n\n'
    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the value envelope'
    mock_case prs-vote-stdin-n "prs vote (stdin n — no prompt)" \
        prs vote Alpha Alpha.Core 137 --vote 10 --json

    status_rule='D34: a missing required option is a silent exit 0 in the oracle; this build is a loud usage error'
    expect_statuses='0 1'
    stdout_mode=text
    mock_case prs-vote-no-option "prs vote (no --vote)" prs vote Alpha Alpha.Core 137 --json

    envelope_rule='D5: the oracle prints the command help on stdout before its usage error; this build writes clap’s message to stderr alone'
    stdout_mode=text
    mock_case prs-vote-non-integer "prs vote (--vote abc)" prs vote Alpha Alpha.Core 137 --vote abc

    # ── Wave 2: the pull request diff (Task 10) ──
    #
    # The captured chain: `--iteration N` is used as given (no list GET), the
    # default is the iteration list's last entry, `GET …/iterations/{n}/changes`
    # is the second request, and only the two content modes re-read the list and
    # fetch the revisions from `…/items`. The scenario's three `items` routes match
    # on the `version` pair, so the base (`aaaa1111`) and target (`cccc3333`)
    # revisions carry the listener capture's own bytes (`prs_item.txt` and
    # `prs_item_target.txt`) and the `--file`/`--unified` cases render real hunks
    # instead of an empty edit; `bbbb2222` (iteration 1's source) is served the base
    # bytes, so the iteration-1 case keeps its deliberately empty hunk. The
    # `/diffs/commits` add/delete entries are whole-file diffs either way. The four
    # `--json`
    # documents are the frozen `render_file_list`/`emit_diff_or_json`/
    # `render_unified` shapes and MATCH, as do the human diff bytes (the listener
    # capture's). Ruled here: the default view's human table is this build's own
    # (D37), the refusals and guards that reach `halt_error` are D4, the 404 body
    # is D24, the iteration rows are D34/D5, and the frozen `Helpers.bail`
    # `network_error` class for a local guard is D36.

    mock_case prs-diff "prs diff" prs diff Alpha Alpha.Core 137 --json

    envelope_rule='D37: the frozen default view hand-rolls a 50/10 table under a `PR diff (iteration N)` header and a `N file(s) changed, +X -Y` footer; this build renders its own table (spec §8) and keeps the iteration and totals in the envelope'
    stdout_mode=text
    mock_case prs-diff-human "prs diff (human)" prs diff Alpha Alpha.Core 137

    mock_case prs-diff-file "prs diff --file /src/app.ex" prs diff Alpha Alpha.Core 137 --file /src/app.ex --json

    stdout_mode=text
    mock_case prs-diff-file-bare "prs diff --file src/app.ex (no leading slash)" \
        prs diff Alpha Alpha.Core 137 --file src/app.ex

    envelope_rule='D4: the frozen CLI writes the guard to stderr with no envelope under --json where this build emits the error envelope'
    mock_case prs-diff-file-rename "prs diff --file (a rename's old path)" \
        prs diff Alpha Alpha.Core 137 --file /renamed/old.ex --json

    envelope_rule='D4: the frozen CLI writes the guard to stderr with no envelope under --json where this build emits the error envelope'
    mock_case prs-diff-file-missing "prs diff --file (no match)" \
        prs diff Alpha Alpha.Core 137 --file src/nope.ex --json

    mock_case prs-diff-unified "prs diff --unified" prs diff Alpha Alpha.Core 137 --unified --json

    stdout_mode=text
    mock_case prs-diff-unified-human "prs diff --unified (human)" \
        prs diff Alpha Alpha.Core 137 --unified

    mock_case prs-diff-iteration-1 "prs diff --iteration 1" \
        prs diff Alpha Alpha.Core 137 --iteration 1 --json

    mock_case prs-diff-iteration-2 "prs diff --iteration 2" \
        prs diff Alpha Alpha.Core 137 --iteration 2 --json

    mock_case prs-diff-iteration-1-file "prs diff --iteration 1 --file /src/first.ex" \
        prs diff Alpha Alpha.Core 137 --iteration 1 --file /src/first.ex --json

    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with inspect/2'
    mock_case prs-diff-iteration-99 "prs diff --iteration 99" \
        prs diff Alpha Alpha.Core 137 --iteration 99 --json

    status_rule='D34: an iteration below 1 has no resolve_iteration/2 clause and the oracle exits 0 silently; this build is a loud usage error'
    expect_statuses='0 1'
    stdout_mode=text
    mock_case prs-diff-iteration-0 "prs diff --iteration 0" \
        prs diff Alpha Alpha.Core 137 --iteration 0 --json

    envelope_rule='D5: the oracle prints the command help on stdout before its usage error; this build writes clap’s message to stderr alone'
    stdout_mode=text
    mock_case prs-diff-iteration-abc "prs diff --iteration abc" \
        prs diff Alpha Alpha.Core 137 --iteration abc

    envelope_rule='D5: the oracle prints the command help on stdout before its usage error; this build writes clap’s message to stderr alone'
    stdout_mode=text
    mock_case prs-diff-no-id "prs diff (no pr_id)" prs diff Alpha Alpha.Core

    envelope_rule='D4: the frozen CLI writes the guard to stderr with no envelope under --json where this build emits the error envelope'
    mock_case prs-diff-both "prs diff --file --unified" \
        prs diff Alpha Alpha.Core 137 --file /src/app.ex --unified --json

    envelope_rule='D4: the frozen CLI writes the guard to stderr with no envelope under --json where this build emits the error envelope'
    mock_case prs-diff-no-iterations "prs diff (no iterations)" \
        prs diff Alpha Alpha.Core 138 --json

    mock_case prs-diff-empty "prs diff (empty change list)" \
        prs diff Alpha Alpha.Core 141 --json

    envelope_rule='D37: the frozen default view hand-rolls a 50/10 table under a `PR diff (iteration N)` header and a `N file(s) changed, +X -Y` footer; this build renders its own table (spec §8) and keeps the iteration and totals in the envelope'
    stdout_mode=text
    mock_case prs-diff-empty-human "prs diff (empty change list, human)" \
        prs diff Alpha Alpha.Core 141

    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with inspect/2'
    mock_case prs-diff-404 "prs diff (404)" prs diff Alpha Alpha.Core 140 --json

    envelope_rule='D36: the frozen Helpers.bail/2 catch-all classifies a local guard as network_error (`Request failed: "…"`); this build classifies by meaning — api_error for a malformed iteration, not_found for a file absent in a commit'
    mock_case prs-diff-missing-commit "prs diff (iteration without a source commit)" \
        prs diff Alpha Alpha.Core 139 --file /src/app.ex --json

    # ── Wave 2: the pull request comment threads (Task 11a) ──
    #
    # Captured shapes: `list`/`update` spell the path `pullRequests` (capital R),
    # `add`/`delete`/`resolve` spell it `pullrequests`; `update` PATCHes the
    # thread and/or the comment (the thread first when both), `add` replies when
    # `--thread-id` is given and attaches a file/line when `--file-path` and
    # `--line` are, `delete` closes the thread or DELETEs the comment, and
    # `resolve` PATCHes the status unvalidated (captured: `--status bogus` goes to
    # the wire). The writes' `--json` documents are the frozen ones and are
    # mirrored (D38) — the frozen CLI already emits a document there, unlike
    # D33's prose paths — and `update --dry-run` prints its actions document even
    # without `--json`, sending nothing. D38's "the response's id where a response
    # exists" half is discriminated by the routes, not just by the captures: on the
    # both-flags path the scenario answers thread 9 with `{"id": 900, …}` and its
    # comment with `{"id": 901, …}`, so the document (and the human lines) must say
    # 900/901 where the arguments are 9/4 — an implementation that always used the
    # arguments fails `prs comments update --content --status` by name. `delete` is
    # the wave's third prompting
    # command: its question is `Close thread N? [y/N] ` / `Close comment N in
    # thread N? [y/N] `, and the oracle answers a refusal with `Cancelled.` on
    # stdout and exit 0 (R2/D30/D32): this build refuses on stderr with exit 1.
    # The invocations table's underscore spellings are rejected by the frozen
    # parser (D17); `add` without `--content` proceeds with an empty body there
    # where this build's clap is loud (D34's class).

    mock_case prs-comments-list "prs comments list" \
        prs comments list Alpha Alpha.Core 137 --json

    stdout_mode=text
    mock_case prs-comments-list-human "prs comments list (human)" \
        prs comments list Alpha Alpha.Core 137

    stdout_mode=text
    mock_case prs-comments-list-all-human "prs comments list --all (human)" \
        prs comments list Alpha Alpha.Core 137 --all

    mock_case prs-comments-list-empty "prs comments list (empty)" \
        prs comments list Alpha Alpha.Core 8 --json

    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with inspect/2'
    mock_case prs-comments-list-404 "prs comments list (404)" \
        prs comments list Alpha Alpha.Core 999 --json

    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with inspect/2 (C2: the list-error row)'
    mock_case prs-comments-list-500 "prs comments list (500)" \
        prs comments list Alpha Alpha.Core 500 --json

    mock_case prs-comments-add "prs comments add" \
        prs comments add Alpha Alpha.Core 137 --content 'Looks good' --json

    mock_case prs-comments-add-inline "prs comments add --file-path --line --end-line" \
        prs comments add Alpha Alpha.Core 140 --content 'Range note' --file-path src/foo.ex \
        --line 3 --end-line 5 --json

    mock_case prs-comments-add-reply "prs comments add --thread-id" \
        prs comments add Alpha Alpha.Core 137 --content 'Reply here.' --thread-id 7 --json

    mock_case prs-comments-add-reply-comment-id "prs comments add --thread-id --comment-id" \
        prs comments add Alpha Alpha.Core 137 --content 'Reply to 3' --thread-id 8 --comment-id 3 --json

    mock_case prs-comments-add-inline-single "prs comments add --file-path --line" \
        prs comments add Alpha Alpha.Core 139 --content 'Inline note' --file-path src/foo.ex \
        --line 3 --json

    mock_case prs-comments-add-inline-leading-slash "prs comments add --file-path with a leading slash" \
        prs comments add Alpha Alpha.Core 141 --content 'Slash note' --file-path /src/bar.ex \
        --line 1 --json

    mock_case prs-comments-add-status "prs comments add --status wontFix" \
        prs comments add Alpha Alpha.Core 142 --content 'By design' --status wontFix --json

    stdout_mode=text
    mock_case prs-comments-add-human "prs comments add (human)" \
        prs comments add Alpha Alpha.Core 137 --content 'Looks good'

    status_rule='D34: the oracle proceeds with an empty body when --content is absent; this build is a loud usage error'
    expect_statuses='0 1'
    rest_rule='D34: the oracle sends the empty-content thread where this build refuses before the request'
    envelope_rule='D34: the oracle sends an empty comment where this build refuses before the request'
    stdout_mode=text
    mock_case prs-comments-add-no-content "prs comments add (no --content)" \
        prs comments add Alpha Alpha.Core 138 --json

    envelope_rule='D4: the frozen CLI writes the guard to stderr with no envelope under --json where this build emits the error envelope'
    mock_case prs-comments-add-invalid-status "prs comments add (--status bogus)" \
        prs comments add Alpha Alpha.Core 137 --content X --status bogus --json

    envelope_rule='D4: the frozen CLI writes the guard to stderr with no envelope under --json where this build emits the error envelope'
    mock_case prs-comments-add-missing-file "prs comments add (missing @file)" \
        prs comments add Alpha Alpha.Core 137 --content @/nonexistent/file.md --json

    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with inspect/2'
    mock_case prs-comments-add-404 "prs comments add (404)" \
        prs comments add Alpha Alpha.Core 999 --content X --json

    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with inspect/2'
    mock_case prs-comments-add-400 "prs comments add (400)" \
        prs comments add Broken Alpha.Core 137 --content X --json

    mock_case prs-comments-update-content "prs comments update --content" \
        prs comments update Alpha Alpha.Core 137 7 3 --content 'Edited text' --json

    mock_case prs-comments-update-status "prs comments update --status" \
        prs comments update Alpha Alpha.Core 137 8 3 --status fixed --json

    mock_case prs-comments-update-both "prs comments update --content --status" \
        prs comments update Alpha Alpha.Core 137 9 4 --content 'Both edited' --status fixed --json

    stdout_mode=text
    mock_case prs-comments-update-both-human "prs comments update --content --status (human)" \
        prs comments update Alpha Alpha.Core 137 9 4 --content 'Both edited' --status fixed

    mock_case prs-comments-update-resolved-by-me "prs comments update --resolved-by-me" \
        prs comments update Alpha Alpha.Core 137 10 3 --status fixed --resolved-by-me --json

    mock_case prs-comments-update-file "prs comments update --content @file" \
        prs comments update Alpha Alpha.Core 137 11 5 --content "@$work/comments-body.md" --json

    case_stdin=$'Stdin body\n\n'
    mock_case prs-comments-update-stdin "prs comments update --content -" \
        prs comments update Alpha Alpha.Core 137 12 6 --content - --json

    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with inspect/2'
    mock_case prs-comments-update-404 "prs comments update (404)" \
        prs comments update Alpha Alpha.Core 137 13 99 --content X --json

    mock_case prs-comments-update-dry-run "prs comments update --dry-run" \
        prs comments update Alpha Alpha.Core 137 9 4 --content 'Both edited' --status fixed --dry-run

    envelope_rule='D4: the frozen CLI writes the guard to stderr with no envelope under --json where this build emits the error envelope'
    mock_case prs-comments-update-no-flags "prs comments update (no --content/--status)" \
        prs comments update Alpha Alpha.Core 137 7 3 --json

    envelope_rule='D4: the frozen CLI writes the guard to stderr with no envelope under --json where this build emits the error envelope'
    mock_case prs-comments-update-invalid-status "prs comments update (--status bogus)" \
        prs comments update Alpha Alpha.Core 137 7 3 --status bogus --json

    envelope_rule='D5: the oracle prints the command help on stdout before its usage error; this build writes clap’s message to stderr alone'
    stdout_mode=text
    mock_case prs-comments-update-underscore "prs comments update --dry_run (unknown flag)" \
        prs comments update Alpha Alpha.Core 137 7 3 --content X --dry_run

    envelope_rule='D5: the oracle prints the command help on stdout before its usage error; this build writes clap’s message to stderr alone'
    stdout_mode=text
    mock_case prs-comments-update-no-comment-id "prs comments update (no comment_id)" \
        prs comments update Alpha Alpha.Core 137 7 --content X --json

    mock_case prs-comments-delete-force "prs comments delete --force" \
        prs comments delete Alpha Alpha.Core 137 21 --force --json

    mock_case prs-comments-delete-comment "prs comments delete --comment-id --force" \
        prs comments delete Alpha Alpha.Core 137 25 --comment-id 4 --force --json

    case_stdin=$'y\n'
    stdout_mode=prompt-text
    mock_case prs-comments-delete-comment-confirmed "prs comments delete --comment-id (confirmed)" \
        prs comments delete Alpha Alpha.Core 137 24 --comment-id 3

    case_stdin=$'y\n'
    stdout_mode=prompt-json
    mock_case prs-comments-delete-confirmed "prs comments delete (confirmed)" \
        prs comments delete Alpha Alpha.Core 137 20 --json

    case_stdin=$'y\n'
    stdout_mode=prompt-text
    mock_case prs-comments-delete-confirmed-human "prs comments delete (confirmed, human)" \
        prs comments delete Alpha Alpha.Core 137 20

    case_stdin=$'n\n'
    status_rule='D32: the oracle answers a refusal with `Cancelled.` on stdout and exit 0; this build refuses on stderr with exit 1'
    expect_statuses='0 1'
    envelope_rule='D32: the oracle prints its refusal on stdout even under --json; this build writes the refusal to stderr and leaves stdout empty'
    stdout_mode=prompt-text
    mock_case prs-comments-delete-refused "prs comments delete (refused)" \
        prs comments delete Alpha Alpha.Core 137 26 --json

    status_rule='D30/D32: the oracle exits 0 on an unanswered prompt; this build refuses on stderr with exit 1'
    expect_statuses='0 1'
    stdout_mode=prompt-text
    mock_case prs-comments-delete-eof "prs comments delete (EOF)" \
        prs comments delete Alpha Alpha.Core 137 26

    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with inspect/2'
    mock_case prs-comments-delete-404 "prs comments delete (404)" \
        prs comments delete Alpha Alpha.Core 137 23 --force --json

    mock_case prs-comments-resolve "prs comments resolve" \
        prs comments resolve Alpha Alpha.Core 137 5 --json

    stdout_mode=text
    mock_case prs-comments-resolve-human "prs comments resolve (human)" \
        prs comments resolve Alpha Alpha.Core 137 5

    mock_case prs-comments-resolve-status "prs comments resolve --status wontFix" \
        prs comments resolve Alpha Alpha.Core 137 6 --status wontFix --json

    mock_case prs-comments-resolve-active "prs comments resolve --status active" \
        prs comments resolve Alpha Alpha.Core 137 14 --status active --json

    mock_case prs-comments-resolve-resolved-by-me "prs comments resolve --resolved-by-me" \
        prs comments resolve Alpha Alpha.Core 137 4 --resolved-by-me --json

    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with inspect/2'
    mock_case prs-comments-resolve-404 "prs comments resolve (404)" \
        prs comments resolve Alpha Alpha.Core 137 15 --json

    case_org=conn-broken
    envelope_rule='D4: the frozen CLI writes the guard to stderr with no envelope under --json where this build emits the error envelope'
    mock_case prs-comments-resolve-conn-broken "prs comments resolve (connectionData 404)" \
        prs comments resolve Alpha Alpha.Core 137 5 --resolved-by-me --json

    # ── Wave 2: the pull request reviewers (Task 11b) ──
    #
    # Captured shapes: all three leaves spell the path `pullrequests` (lower
    # case, like `vote`); `--reviewer` addresses the item route *and* is the
    # body's `id` (`isRequired: true` only under `--required`); `--search` is the
    # module's client-side fuzzy filter over `displayName` and `uniqueName`
    # (substring or subsequence, case-insensitive; absent or empty is no filter —
    # the `aae` case is the capture that shows the filter reads `uniqueName`
    # too). `list` is the first consumer of the oracle's
    # `{"ok":true,"count":N,"items":[…]}` document, mirrored here as `ok_list`
    # (C4 closes). The writes' `--json` output is this build's value/message
    # envelope (D33 where the oracle prints its human success line in both
    # modes), the two 404s are D4 and the 4xx/5xx bodies D24; `remove`'s non-404
    # fallback is D4's class as well — the oracle prints `xx  Remove failed: …`
    # prose on **stdout** with no envelope, this build returns the client's
    # classified error. A missing `--reviewer` is D34's silent exit 0 in the
    # oracle (the swallowed `Map.fetch!`), loud here. No underscore spelling is
    # rejected by the frozen parser in this group. The human views are this
    # build's own §8 surface — the non-empty table (like Wave 1's list tables)
    # and the empty sentence — and carry no case; the integration suite pins the
    # table's columns and the sentence, and the captures are
    # `captures/task11b/oracle/{list-human,list-empty-human}.out`.

    mock_case prs-reviewers-list "prs reviewers list" \
        prs reviewers list Alpha Alpha.Core 137 --json

    mock_case prs-reviewers-list-search "prs reviewers list --search ada" \
        prs reviewers list Alpha Alpha.Core 137 --search ada --json

    mock_case prs-reviewers-list-search-subsequence "prs reviewers list --search aae (subsequence)" \
        prs reviewers list Alpha Alpha.Core 137 --search aae --json

    mock_case prs-reviewers-list-search-no-match "prs reviewers list --search zzz" \
        prs reviewers list Alpha Alpha.Core 137 --search zzz --json

    mock_case prs-reviewers-list-empty "prs reviewers list (empty)" \
        prs reviewers list Alpha Alpha.Core 8 --json

    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with inspect/2'
    mock_case prs-reviewers-list-404 "prs reviewers list (404)" \
        prs reviewers list Alpha Alpha.Core 999 --json

    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with inspect/2 (C2: the list-error row)'
    mock_case prs-reviewers-list-500 "prs reviewers list (500)" \
        prs reviewers list Alpha Alpha.Core 500 --json

    envelope_rule='D5: the oracle prints the command help on stdout before its usage error; this build writes clap’s message to stderr alone'
    stdout_mode=text
    mock_case prs-reviewers-list-no-pr-id "prs reviewers list (no pr_id)" \
        prs reviewers list Alpha Alpha.Core --json

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the value envelope'
    mock_case prs-reviewers-add "prs reviewers add" \
        prs reviewers add Alpha Alpha.Core 137 \
        --reviewer aaaaaaaa-0001-0001-0001-000000000001 --json

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the value envelope'
    mock_case prs-reviewers-add-required "prs reviewers add --required" \
        prs reviewers add Alpha Alpha.Core 139 \
        --reviewer bbbbbbbb-0002-0002-0002-000000000002 --required --json

    stdout_mode=text
    mock_case prs-reviewers-add-human "prs reviewers add (human)" \
        prs reviewers add Alpha Alpha.Core 137 \
        --reviewer aaaaaaaa-0001-0001-0001-000000000001

    rest_rule='D22: the reviewer id is a path segment here, percent-encoded more strictly than the frozen URI.encode/1 (which left the @ alone), so the request paths differ and the bodies do not'
    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the value envelope'
    mock_case prs-reviewers-add-email "prs reviewers add --reviewer email" \
        prs reviewers add Alpha Alpha.Core 137 --reviewer ada@example.com --json

    envelope_rule='D4: the frozen CLI writes the guard to stderr with no envelope under --json where this build emits the error envelope'
    mock_case prs-reviewers-add-404 "prs reviewers add (404)" \
        prs reviewers add Alpha Alpha.Core 999 \
        --reviewer aaaaaaaa-0001-0001-0001-000000000001 --json

    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with inspect/2'
    mock_case prs-reviewers-add-400 "prs reviewers add (400)" \
        prs reviewers add Broken Alpha.Core 137 \
        --reviewer aaaaaaaa-0001-0001-0001-000000000001 --json

    status_rule='D34: a missing required option is a silent exit 0 in the oracle; this build is a loud usage error'
    expect_statuses='0 1'
    stdout_mode=text
    mock_case prs-reviewers-add-no-reviewer "prs reviewers add (no --reviewer)" \
        prs reviewers add Alpha Alpha.Core 137 --json

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the message envelope'
    mock_case prs-reviewers-remove "prs reviewers remove" \
        prs reviewers remove Alpha Alpha.Core 137 \
        --reviewer aaaaaaaa-0001-0001-0001-000000000001 --json

    stdout_mode=text
    mock_case prs-reviewers-remove-human "prs reviewers remove (human)" \
        prs reviewers remove Alpha Alpha.Core 137 \
        --reviewer aaaaaaaa-0001-0001-0001-000000000001

    envelope_rule='D4: the frozen CLI writes the guard to stderr with no envelope under --json where this build emits the error envelope'
    mock_case prs-reviewers-remove-404 "prs reviewers remove (404)" \
        prs reviewers remove Alpha Alpha.Core 999 \
        --reviewer aaaaaaaa-0001-0001-0001-000000000001 --json

    envelope_rule='D4: the frozen CLI writes no envelope for this failure — `xx  Remove failed: …` prose on stdout here — where this build emits the error envelope'
    mock_case prs-reviewers-remove-500 "prs reviewers remove (500)" \
        prs reviewers remove Alpha Alpha.Core 137 \
        --reviewer bbbbbbbb-0002-0002-0002-000000000002 --json

    status_rule='D34: a missing required option is a silent exit 0 in the oracle; this build is a loud usage error'
    expect_statuses='0 1'
    stdout_mode=text
    mock_case prs-reviewers-remove-no-reviewer "prs reviewers remove (no --reviewer)" \
        prs reviewers remove Alpha Alpha.Core 137 --json

    mock_scenario_check
}

if [[ $mode == mock ]]; then
    run_mock_cases
    summary
    exit 0
fi

# ── version ──────────────────────────────────────────────────────────────

start_case "version"
capture version version

if exits_ok version; then
    el=$(cat "$work/version.elixir")
    rs=$(cat "$work/version.rust")

    if [[ ! $el =~ ^ado\ [^[:space:]]+$ ]]; then
        fail "the oracle prints '$el', expected 'ado <version>'"
    elif [[ ! $rs =~ ^ado\ [^[:space:]]+$ ]]; then
        fail "the Rust binary prints '$rs', expected 'ado <version>'"
    elif [[ $el == "$rs" ]]; then
        note "both print '$el'"
    else
        ruled "version value: '$el' vs '$rs'"
    fi
fi

finish_case

# ── version --json ───────────────────────────────────────────────────────

start_case "version --json"
capture version-json version --json

if exits_ok version-json &&
    json_ok "$work/version-json.elixir" '.ok == true and (.version | type == "string")' 'the oracle is not {"ok":true,"version":"…"}' &&
    json_ok "$work/version-json.rust" '.ok == true and (.version | type == "string")' 'the Rust envelope is not {"ok":true,"version":"…"}'; then

    jq -S 'del(.version)' "$work/version-json.elixir" >"$work/version-json.elixir.norm"
    jq -S 'del(.version)' "$work/version-json.rust" >"$work/version-json.rust.norm"

    el_version=$(jq -r '.version' "$work/version-json.elixir")
    rs_version=$(jq -r '.version' "$work/version-json.rust")

    if ! same "$work/version-json.elixir.norm" "$work/version-json.rust.norm"; then
        fail "the envelope differs beyond the version value: $(first_difference "$work/version-json.elixir.norm" "$work/version-json.rust.norm")"
    elif [[ $el_version == "$rs_version" ]]; then
        note "identical, version $el_version"
    else
        ruled "version value: $el_version vs $rs_version"
    fi
fi

finish_case

# ── whoami --json ────────────────────────────────────────────────────────

start_case "whoami --json"
capture whoami-json whoami --json

if exits_ok whoami-json &&
    json_ok "$work/whoami-json.elixir" '.ok == true and (.result | type == "object")' 'the oracle is not {"ok":true,"result":{…}}' &&
    json_ok "$work/whoami-json.rust" '.ok == true and (.result | type == "object")' 'the Rust envelope is not {"ok":true,"result":{…}}'; then

    # The result keys are contract; only the config path may differ.
    jq -S '.result | keys' "$work/whoami-json.elixir" >"$work/whoami-json.keys.elixir"
    jq -S '.result | keys' "$work/whoami-json.rust" >"$work/whoami-json.keys.rust"

    if ! same "$work/whoami-json.keys.elixir" "$work/whoami-json.keys.rust"; then
        fail "the result keys differ: $(first_difference "$work/whoami-json.keys.elixir" "$work/whoami-json.keys.rust")"
    else
        note "result keys: $(tr -d '\n ' <"$work/whoami-json.keys.rust")"
    fi

    jq -S 'del(.result.config_file)' "$work/whoami-json.elixir" >"$work/whoami-json.elixir.norm"
    jq -S 'del(.result.config_file)' "$work/whoami-json.rust" >"$work/whoami-json.rust.norm"

    if ! same "$work/whoami-json.elixir.norm" "$work/whoami-json.rust.norm"; then
        fail "the result differs beyond the config path: $(first_difference "$work/whoami-json.elixir.norm" "$work/whoami-json.rust.norm")"
    fi

    # The two paths prove the isolated homes were used, and that the legacy
    # ~/.ado_cli/config.json is not what the Rust binary reads.
    el_config=$(jq -r '.result.config_file' "$work/whoami-json.elixir")
    rs_config=$(jq -r '.result.config_file' "$work/whoami-json.rust")

    if [[ $el_config != "$work/home-elixir/.ado_cli/config.json" ]]; then
        fail "the oracle config path is '$el_config', not the isolated ~/.ado_cli/config.json"
    elif [[ $rs_config != "$work/home-rust/"*"/ado/config.toml" ]]; then
        fail "the Rust config path is '$rs_config', not <config dir>/ado/config.toml under the isolated home"
    elif same "$work/whoami-json.elixir.norm" "$work/whoami-json.rust.norm"; then
        ruled "config_file: $el_config vs $rs_config"
    fi
fi

finish_case

# ── schema --json ────────────────────────────────────────────────────────

start_case "schema --json"
capture schema-json schema --json

schema_el="$work/schema-json.elixir"
schema_rs="$work/schema-json.rust"

if exits_ok schema-json &&
    json_ok "$schema_el" '.ok == true and (.schema | type == "object")' 'the oracle is not {"ok":true,"schema":{…}}' &&
    json_ok "$schema_rs" '.ok == true and (.schema | type == "object")' 'the Rust envelope is not {"ok":true,"schema":{…}}' &&
    json_ok "$schema_el" '.schema.version | type == "string"' 'the oracle node has no version key' &&
    json_ok "$schema_rs" '.schema.version | type == "string"' 'the Rust node has no version key'; then

    # Two Wave 1 differences are normalised away before the comparisons below, so
    # that everything else — types, defaults, arguments, option spelling, the tree
    # shape — is still compared exactly:
    #
    #   * D18/R3: the group nodes this build reports in the spelling argv accepts
    #     differ from the oracle's display names — `pipelines builds` and `pipelines
    #     artifacts` gain a hyphen, `pipelines secure-files` loses the hyphen for the
    #     underscore, and `pipelines folders` gains the hyphen — at the root and in
    #     every descendant's name;
    #   * doc values are §8 regenerated surface, and the nodes this wave wrote or
    #     rewrote carry this build's wording. Their option and argument docs are
    #     blanked here; their node docs are reported by the doc check further down.
    s8_nodes='["ado login", "ado prs", "ado workitems create"]'
    globals_names=$(jq -c '[.schema.options[].name]' "$schema_rs")
    d18_and_s8="walk(if type == \"object\" and (.name? | type) == \"string\"
        then .name |= gsub(\" pipelines builds\"; \" pipelines-builds\")
                   | .name |= gsub(\" pipelines artifacts\"; \" pipelines-artifacts\")
                   | .name |= gsub(\" pipelines secure_files\"; \" pipelines secure-files\")
                   | .name |= gsub(\" pipelines folders\"; \" pipelines-folders\")
        else . end)
      | (.schema.subcommands[] | select(.name as \$node | $s8_nodes | index(\$node))
         | .options[]? | select(.name as \$o | $globals_names | index(\$o) | not) | .doc) |= \"\"
      | (.schema.subcommands[] | select(.name as \$node | $s8_nodes | index(\$node))
         | .arguments[]? | .doc) |= \"\""

    jq -S "$d18_and_s8" "$schema_el" >"$work/schema.elixir"
    jq -S "$d18_and_s8" "$schema_rs" >"$work/schema.rust"
    schema_el="$work/schema.elixir"
    schema_rs="$work/schema.rust"

    # D18/R3's premise: the oracle spells these group nodes differently from the
    # spelling argv accepts. If that changed, this is not the frozen oracle and the
    # normalisation above is stale.
    d18_renamed=$(jq -r '[.schema | recurse(.subcommands[]?) | .name | select(test(" pipelines (builds|artifacts|secure-files|folders)"))] | length' "$work/schema-json.elixir")

    if (( d18_renamed > 0 )); then
        note "D18/R3: $d18_renamed oracle node names differ from the runnable spelling this build reports"
    else
        fail "the oracle no longer spells these group nodes differently from argv (D18/R3)"
    fi

    # Node shape (§1.1): the returned node carries `version`, the rest do not.
    jq -S '.schema | keys' "$schema_el" >"$work/schema.keys.elixir"
    jq -S '.schema | keys' "$schema_rs" >"$work/schema.keys.rust"

    if same "$work/schema.keys.elixir" "$work/schema.keys.rust"; then
        note "root keys: $(tr -d '\n ' <"$work/schema.keys.rust")"
    else
        fail "the root node keys differ: $(first_difference "$work/schema.keys.elixir" "$work/schema.keys.rust")"
    fi

    # The root node minus the version value.
    root_projection='{name: .schema.name, doc: .schema.doc, arguments: .schema.arguments, options: (.schema.options | sort_by(.name))}'
    jq -S "$root_projection" "$schema_el" >"$work/schema.root.elixir"
    jq -S "$root_projection" "$schema_rs" >"$work/schema.root.rust"

    if same "$work/schema.root.elixir" "$work/schema.root.rust"; then
        note "root node identical (name, doc, arguments, the five globals)"
    else
        fail "the root node differs: $(first_difference "$work/schema.root.elixir" "$work/schema.root.rust")"
    fi

    el_version=$(jq -r '.schema.version' "$schema_el")
    rs_version=$(jq -r '.schema.version' "$schema_rs")

    if [[ $el_version == "$rs_version" ]]; then
        note "version value: $el_version on both"
    else
        ruled "version value: $el_version vs $rs_version"
    fi

    # Every Rust node, at every depth, must be a node the oracle has: the tree only
    # shrinks (§10), and D18's spelling is normalised away above. Depth matters from
    # Wave 1 on, because the group nodes bring children this wave did not port.
    jq -r '.schema | recurse(.subcommands[]?) | .name' "$schema_el" | sort -u >"$work/schema.names.elixir"
    jq -r '.schema | recurse(.subcommands[]?) | .name' "$schema_rs" | sort -u >"$work/schema.names.rust"
    unlisted=$(comm -13 "$work/schema.names.elixir" "$work/schema.names.rust" | tr '\n' ' ' | sed 's/ *$//')

    el_count=$(jq '.schema | recurse(.subcommands[]?) | .name' "$schema_el" | sort -u | wc -l | tr -d ' ')
    rs_count=$(jq '.schema | recurse(.subcommands[]?) | .name' "$schema_rs" | sort -u | wc -l | tr -d ' ')

    if [[ $rs_count == 0 ]]; then
        fail "the Rust tree has no subcommands"
    elif [[ -n $unlisted ]]; then
        fail "the Rust tree names nodes the oracle does not: $unlisted"
    else
        note "every Rust node is an oracle node ($rs_count of $el_count paths)"
        ruled "unported commands absent: $rs_count of $el_count nodes (§10)"
    fi

    # The oracle's frozen tree lists `ado security` twice (D14). If that
    # changed, this is not the frozen oracle.
    duplicates=$(jq -r '.schema.subcommands[].name' "$schema_el" | sort | uniq -d | tr '\n' ' ' | sed 's/ *$//')

    if [[ $duplicates == "ado security" ]]; then
        note "the oracle still lists 'ado security' twice (D14)"
    else
        fail "the oracle's duplicate subcommands are '$duplicates', not 'ado security' (D14)"
    fi

    # Every Rust subcommand node carries the root's globals (D2).
    globals_entries=$(jq -cS '[.schema.options[]] | sort_by(.name)' "$schema_rs")

    while IFS= read -r node; do
        node_globals=$(jq -cS --arg node "$node" --argjson names "$globals_names" \
            '[.schema.subcommands[] | select(.name == $node) | .options[] | select(.name as $o | $names | index($o))] | sort_by(.name)' "$schema_rs")

        if [[ $node_globals != "$globals_entries" ]]; then
            fail "node '$node' carries different globals than the root (D2): $node_globals"
        fi
    done < <(jq -r '.schema.subcommands[].name' "$schema_rs")

    # The nodes both sides have: name, arguments, the children both sides have and
    # the options that are not globals. Docs are compared separately, because D10
    # lets the Rust doc be the oracle's truncated at the end of the usage block (and
    # §8 leaves the wording of the nodes this wave wrote free). The only ruled value
    # difference left is the spelling of `write-to-file` (D17), so the raw
    # comparison is expected to fail there and the hyphen/underscore-normalised one
    # is expected to pass. Children are the intersection: an oracle child this wave
    # did not port is the recorded shrink (§10), and a Rust child the oracle does
    # not have fails the name check above.
    shared_names=$(jq -c '[.schema.subcommands[].name]' "$schema_rs")
    shared_children=$(jq -c '[.schema.subcommands[] | {key: .name, value: [.subcommands[].name]}] | from_entries' "$schema_rs")
    node_projection='[ .schema.subcommands[]
        | select(.name as $node | $nodes | index($node))
        | . as $self
        | { name, arguments,
            options: ([ .options[] | select(.name as $o | $names | index($o) | not) ] | sort_by(.name)),
            subcommands: ([ $self.subcommands[].name ]
                | map(select(. as $child | ($children[$self.name] // []) | index($child))) | sort) } ]
      | sort_by(.name)'
    node_projection_normalised='[ .schema.subcommands[]
        | select(.name as $node | $nodes | index($node))
        | . as $self
        | { name, arguments,
            options: ([ .options[]
                | select(.name as $o | $names | index($o) | not)
                | if .name == "write-to-file" then .name = "write_to_file" else . end ] | sort_by(.name)),
            subcommands: ([ $self.subcommands[].name ]
                | map(select(. as $child | ($children[$self.name] // []) | index($child))) | sort) } ]
      | sort_by(.name)'

    jq -S --argjson nodes "$shared_names" --argjson names "$globals_names" --argjson children "$shared_children" "$node_projection" "$schema_el" >"$work/schema.nodes.elixir"
    jq -S --argjson nodes "$shared_names" --argjson names "$globals_names" --argjson children "$shared_children" "$node_projection" "$schema_rs" >"$work/schema.nodes.rust"
    jq -S --argjson nodes "$shared_names" --argjson names "$globals_names" --argjson children "$shared_children" "$node_projection_normalised" "$schema_el" >"$work/schema.nodes.norm.elixir"
    jq -S --argjson nodes "$shared_names" --argjson names "$globals_names" --argjson children "$shared_children" "$node_projection_normalised" "$schema_rs" >"$work/schema.nodes.norm.rust"

    if same "$work/schema.nodes.elixir" "$work/schema.nodes.rust"; then
        note "the shared nodes match exactly"
    elif same "$work/schema.nodes.norm.elixir" "$work/schema.nodes.norm.rust"; then
        ruled "option name spelling (D17): 'write_to_file' vs 'write-to-file'"
    else
        fail "the shared nodes differ beyond the ruled spelling: $(first_difference "$work/schema.nodes.norm.elixir" "$work/schema.nodes.norm.rust")"
    fi

    # Node docs: equal, or this build's §8 wording for the nodes this wave wrote,
    # or the Rust doc is the oracle's prefix (D10).
    while IFS= read -r node; do
        el_doc=$(jq -r --arg node "$node" '.schema.subcommands[] | select(.name == $node) | .doc' "$schema_el")
        rs_doc=$(jq -r --arg node "$node" '.schema.subcommands[] | select(.name == $node) | .doc' "$schema_rs")

        if [[ $el_doc == "$rs_doc" ]]; then
            continue
        elif [[ $s8_nodes == *"\"$node\""* ]]; then
            ruled "node doc is this build's §8 wording: '$node' is '${rs_doc:0:60}' where the oracle has '${el_doc:0:60}'"
        elif [[ $el_doc == "$rs_doc"* ]]; then
            ruled "node doc truncated (D10): '$node' keeps ${#rs_doc} of ${#el_doc} characters"
        else
            fail "node doc rewritten: '$node' is '${rs_doc:0:80}' where the oracle has '${el_doc:0:80}'"
        fi
    done < <(jq -r '.schema.subcommands[].name' "$schema_rs")
fi

finish_case

# ── schema version --json ────────────────────────────────────────────────

start_case "schema version --json"
capture schema-version-json schema version --json
capture schema-version-root schema --json

node_el="$work/schema-version-json.elixir"
node_rs="$work/schema-version-json.rust"

if exits_ok schema-version-json &&
    json_ok "$node_el" '.ok == true and .schema.name == "ado version"' "the oracle node is not 'ado version'" &&
    json_ok "$node_rs" '.ok == true and .schema.name == "ado version"' "the Rust node is not 'ado version'" &&
    json_ok "$node_el" '.schema.version | type == "string"' 'the oracle node has no version key' &&
    json_ok "$node_rs" '.schema.version | type == "string"' 'the Rust node has no version key'; then

    node_shape='.schema | {name, doc, arguments, subcommands: [.subcommands[].name]}'
    jq -S "$node_shape" "$node_el" >"$work/node.shape.elixir"
    jq -S "$node_shape" "$node_rs" >"$work/node.shape.rust"

    if same "$work/node.shape.elixir" "$work/node.shape.rust"; then
        note "node identity identical (name, doc, arguments, subcommands)"
    else
        fail "the node identity differs: $(first_difference "$work/node.shape.elixir" "$work/node.shape.rust")"
    fi

    el_version=$(jq -r '.schema.version' "$node_el")
    rs_version=$(jq -r '.schema.version' "$node_rs")

    if [[ $el_version == "$rs_version" ]]; then
        note "version value: $el_version on both"
    else
        ruled "version value: $el_version vs $rs_version"
    fi

    # The oracle lists the command's own `json` duplicate; Rust lists the five
    # globals clap copies in (D2).
    el_options=$(jq '.schema.options | length' "$node_el")

    if [[ $el_options != 1 ]]; then
        fail "the oracle node has $el_options options, expected its one local option"
    elif [[ $(jq -r '.schema.options[0].name' "$node_el") != "json" ]]; then
        fail "the oracle's local option is '$(jq -r '.schema.options[0].name' "$node_el")', expected 'json'"
    fi

    jq -S '[.schema.options[]] | sort_by(.name)' "$node_rs" >"$work/node.options.rust"
    jq -S '[.schema.options[]] | sort_by(.name)' "$work/schema-version-root.rust" >"$work/node.options.root"

    if same "$work/node.options.rust" "$work/node.options.root"; then
        ruled "globals in every schema node (D2): the oracle lists 1 local option, Rust lists the root's 5"
    else
        fail "the node's options are not the root's globals: $(first_difference "$work/node.options.root" "$work/node.options.rust")"
    fi

    el_json_doc=$(jq -r '.schema.options[] | select(.name == "json") | .doc' "$node_el")
    rs_json_doc=$(jq -r '.schema.options[] | select(.name == "json") | .doc' "$node_rs")

    if [[ $el_json_doc == "$rs_json_doc" ]]; then
        note "the json option doc: '$rs_json_doc' on both"
    else
        ruled "the json option doc (D2): '$el_json_doc' vs '$rs_json_doc'"
    fi
fi

finish_case

# ── completion ───────────────────────────────────────────────────────────

completion_case() {
    local shell=$1 el rs el_header rs_header el_trailing rs_trailing parse_note parse_status

    start_case "completion $shell"
    capture "completion-$shell" completion "$shell"

    el="$work/completion-$shell.elixir"
    rs="$work/completion-$shell.rust"

    if exits_ok "completion-$shell"; then
        if [[ ! -s $el ]]; then
            fail "the oracle emitted nothing"
        elif [[ ! -s $rs ]]; then
            fail "the Rust command emitted nothing"
        elif [[ $(head -c 1 "$el") == "{" ]]; then
            fail "the oracle wrapped the script in an envelope"
        elif [[ $(head -c 1 "$rs") == "{" ]]; then
            fail "the Rust command wrapped the script in an envelope"
        fi

        el_header=$(grep -m1 '^# Generated by: ado completion' "$el" || true)
        rs_header=$(grep -m1 '^# Generated by: ado completion' "$rs" || true)

        if [[ -z $el_header ]]; then
            fail "the oracle emits no provenance line"
        elif [[ -z $rs_header ]]; then
            fail "the Rust script has no provenance line"
        elif [[ $el_header == "$rs_header" ]]; then
            note "provenance line: '$rs_header'"
        else
            ruled "provenance wording (D9b): '${el_header#*: }' vs '${rs_header#*: }'"
        fi

        el_trailing=$(trailing_newlines "$el")
        rs_trailing=$(trailing_newlines "$rs")

        if [[ $rs_trailing == 0 ]]; then
            fail "the Rust script does not end with a newline"
        elif [[ $el_trailing == "$rs_trailing" ]]; then
            note "both end with $rs_trailing trailing newline(s)"
        else
            ruled "trailing newlines (IO.puts, §8): $el_trailing vs $rs_trailing"
        fi

        if cmp -s "$el" "$rs"; then
            note "the scripts are byte-identical ($(wc -c <"$rs" | tr -d ' ') bytes)"
        else
            ruled "generated scripts differ (§8/D9): $(wc -c <"$el" | tr -d ' ') vs $(wc -c <"$rs" | tr -d ' ') bytes"
        fi

        parse_note=$(parse_check "$shell" "$rs")
        parse_status=$?

        case $parse_status in
            0) note "$shell: parses" ;;
            1) fail "$parse_note" ;;
            2) note "$parse_note" ;;
        esac
    fi

    finish_case
}

completion_case bash
completion_case zsh
completion_case fish
completion_case powershell

# ── summary ──────────────────────────────────────────────────────────────

summary
