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
# A mutation case has three additions:
#
#   * **Bodies are contract, and a route can pin one.** A scenario route may carry
#     `request_body` (a string compares the bytes as sent; an object or array
#     compares structurally, so key order and whitespace cannot fail a body the
#     two sides send identically). The mock still routes by method and path and
#     records `body_matched` on the request line, so a body no route pins is still
#     compared between the two sides — and a body a route pins is checked against
#     that pin even when both sides send the same wrong bytes. Either mismatch
#     fails the case.
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
# A case may set `rest_rule`, `envelope_rule`, `status_rule`, `stdout_mode`,
# `case_org`, `case_pat`, `case_extra`, `case_stdin` or `compare_files` immediately
# before it; `mock_case` clears them afterwards, so a rule cannot leak into the next
# case. A case that meets a difference no rule covers is a finding, not a row to
# invent.

mock_bin=${ADO_ORACLE_MOCK:-$root/target/debug/mock}
mock_scenario=${ADO_ORACLE_SCENARIO:-$root/scripts/oracle-mock-scenario.json}
mock_url=
mock_log=
mock_requests=
mock_org=ado-harness
mock_pat=harness-pat

rest_rule=
envelope_rule=
status_rule=
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
    envelope_rule=
    status_rule=
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
        text)
            strip_colour "$work/$slug.elixir" >"$work/$slug.stdout.elixir"
            ;;
        prompt-text | prompt-json)
            mock_prompt_check "$slug" || return
            prompt_strip "$work/$slug.elixir" "$work/$slug.stdout.elixir"
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
# so a route no case exercises cannot hide a missing case.
mock_scenario_check() {
    local missing total

    start_case "scenario coverage"

    jq -r '.responses[] | "\(.method) \(.path)"' "$mock_scenario" | sort -u >"$work/scenario.declared"
    jq -r 'select(.matched) | "\(.method) \(.path)"' "$mock_requests" |
        sed "s|$mock_url|{base}|g" | sort -u >"$work/scenario.exercised"

    total=$(wc -l <"$work/scenario.declared" | tr -d ' ')
    missing=$(comm -13 "$work/scenario.exercised" "$work/scenario.declared" | tr '\n' ' ')
    missing=${missing% }

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
    case_pat=
    mock_case login-blank-pat "login --method pat --pat ''" \
        login --method pat --org "$mock_org" --pat "" --json

    envelope_rule='D16: a blank ADO_PAT leaves no method to infer, so this build refuses the invocation (the message wording is §8)'
    status_rule='D16: a blank ADO_PAT reads as unset here (exit 1) where the frozen CLI infers method=pat from it and exits 0'
    case_pat=
    case_extra=("ADO_PAT=")
    mock_case login-blank-env-pat "login with a blank ADO_PAT" login --json

    envelope_rule='D27c: the message says what was removed instead of the legacy ~/.ado_cli/config.json path'
    mock_case logout "logout" logout --json

    envelope_rule='D27c: the message says what was removed instead of the legacy ~/.ado_cli/config.json path'
    mock_case logout-org "logout --org" logout --org "$mock_org" --json

    # ── Wave 2 mutations ──
    #
    # The mutation cases land with the commands that answer them (Task 3 onward):
    # each sets `case_stdin` for its input, its route carries the `request_body` pin
    # in scripts/oracle-mock-scenario.json, and a prompt path picks
    # `stdout_mode=prompt-json` (the oracle's prompt followed by the envelope) or
    # `prompt-text` (a refusal), with a `status_rule` on the case that captures
    # D30's EOF refusal. The three prompting commands, their question text and the
    # captured cases are recorded in docs/rust-rewrite/contract-inventory.md §5.

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
    #   * D18 hyphenates the two group nodes the oracle spells with spaces, at the
    #     root and in their descendants' names;
    #   * doc values are §8 regenerated surface, and the nodes this wave wrote or
    #     rewrote carry this build's wording. Their option and argument docs are
    #     blanked here; their node docs are reported by the doc check further down.
    s8_nodes='["ado login", "ado prs"]'
    globals_names=$(jq -c '[.schema.options[].name]' "$schema_rs")
    d18_and_s8="walk(if type == \"object\" and (.name? | type) == \"string\"
        then .name |= gsub(\" pipelines builds\"; \" pipelines-builds\")
                   | .name |= gsub(\" pipelines artifacts\"; \" pipelines-artifacts\")
        else . end)
      | (.schema.subcommands[] | select(.name as \$node | $s8_nodes | index(\$node))
         | .options[]? | select(.name as \$o | $globals_names | index(\$o) | not) | .doc) |= \"\"
      | (.schema.subcommands[] | select(.name as \$node | $s8_nodes | index(\$node))
         | .arguments[]? | .doc) |= \"\""

    jq -S "$d18_and_s8" "$schema_el" >"$work/schema.elixir"
    jq -S "$d18_and_s8" "$schema_rs" >"$work/schema.rust"
    schema_el="$work/schema.elixir"
    schema_rs="$work/schema.rust"

    # D18's premise: the oracle spells the two group nodes with spaces. If that
    # changed, this is not the frozen oracle and the normalisation above is stale.
    d18_renamed=$(jq -r '[.schema | recurse(.subcommands[]?) | .name | select(test(" pipelines (builds|artifacts)"))] | length' "$work/schema-json.elixir")

    if (( d18_renamed > 0 )); then
        note "D18: $d18_renamed oracle node names read as the hyphenated spelling this build reports"
    else
        fail "the oracle no longer spells the two group nodes with spaces (D18)"
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
