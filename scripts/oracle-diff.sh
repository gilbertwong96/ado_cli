#!/usr/bin/env bash
# Wave 0 oracle diff — the frozen Elixir `ado` (the oracle) against the Rust
# release binary, for every command Wave 0 ported: `version`, `whoami`, `schema`
# and `completion`.
#
# The oracle is an **untracked, prebuilt artifact** (`./ado`, gitignored): the tree
# at this head can no longer rebuild it — `mix escript.build` died with
# `priv/skills` (D49) — so every run of this harness is one-way evidence from here
# on (`w4-handoff.md` §5 records the recovery recipe and the artifact's hash).
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
# `expect_statuses`, `expect_oracle_requests`, `expect_rust_requests`,
# `stdout_mode`, `case_org`, `case_pat`, `case_extra`, `case_stdin` or
# `compare_files` immediately before it; `mock_case` clears them afterwards, so a
# rule cannot leak into the next case. A case that meets a difference no rule covers
# is a finding, not a row to invent.
#
# `expect_statuses` is the one mechanism that lets a case assert a *direction*
# rather than record an expected difference:
#
#   * `expect_statuses='<oracle> <rust>'` pins the exact pair of exit statuses. A
#     `status_rule` only fires when the two statuses differ, so a case whose point
#     is "ours refuses where the oracle does not" would read MATCH if this build
#     regressed to the oracle's shape. With the pair asserted, that regression
#     fails the case by name.
#
# `rest_norm` asserts more than it forgives, but it is *not* a direction:
#
#   * `rest_norm` names the jq filter that mechanically expresses a `rest_rule`
#     (a query spelling, say). When it is set, the raw request lists may differ
#     only in that way: the filter is applied to both sides' projections and
#     anything still different — a missing or extra request included — fails the
#     case, where a bare `rest_rule` rules the whole request list away. The filter
#     is symmetric, so it cannot say which side carried the difference.
#
# `expect_oracle_requests` and `expect_rust_requests` are the request-direction
# assertion (C6): each is a jq filter over **one side's** recorded requests, asserted
# whenever the case sets it — whether or not the two sides differ. Unlike every rule
# above, which is consulted only after the two projections differ, it can fail on a
# case that would otherwise read MATCH. It is adopted on the 23 cases whose request
# difference is a spelling: on 14 of them that spelling is the case's only
# difference, so a candidate regressing to the oracle's spelling would normalise
# equal and pass; on the other 9 an envelope rule still fires on another difference,
# but the request rule would stop firing.
#
#   * The filter reads the side's request log as one array (`jq -s`): the mock's log
#     objects in arrival order, `{method, path, query (exactly as sent, undecoded),
#     body (as sent), matched, body_matched}`.
#   * A failing filter fails the case and names the side; a satisfied one is silent,
#     so an untouched run's output is byte-identical to the run before the adoption.
#   * The `direction_jq` prelude below is prepended to every filter: `qpair("p")`
#     asserts a request carried exactly the pair `p`, `any_path("n")`/`any_body("n")`
#     that a request's path/body contains `n`. Anything else is raw jq over the log
#     objects. Both sides' spellings are stated, so the case's premise cannot drift
#     silently on either side.

mock_bin=${ADO_ORACLE_MOCK:-$root/target/debug/mock}
mock_scenario=${ADO_ORACLE_SCENARIO:-$root/scripts/oracle-mock-scenario.json}
mock_url=
mock_log=
mock_requests=
mock_org=ado-harness
mock_pat=harness-pat
forbid_commands=()

rest_rule=
rest_norm=
envelope_rule=
status_rule=
expect_statuses=
expect_oracle_requests=
expect_rust_requests=
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

    # A case can forbid a command (`forbid_commands`): the command is shadowed by a
    # shim on this run's PATH that records the invocation and refuses, so the flow
    # the case guards against cannot start a real program, and mock_forbidden_check
    # fails the case by name when the shim is reached.
    if (( ${#forbid_commands[@]} > 0 )); then
        local shim=$work/shims/$name.$side command
        mkdir -p "$shim"
        for command in "${forbid_commands[@]}"; do
            printf '#!/usr/bin/env bash\n# The case forbids this command: record the invocation and refuse.\ntouch "$(dirname -- "$0")/used.%s"\nexit 1\n' "$command" >"$shim/$command"
            chmod +x "$shim/$command"
        done
        environment+=("PATH=$shim:$PATH")
    fi

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
    mock_forbidden_check "$slug"
    mock_exit_check "$slug"
    mock_requests_check "$slug"
    mock_direction_check "$slug"
    mock_stdout_check "$slug"
    mock_files_check "$slug"

    rest_rule=
    rest_norm=
    envelope_rule=
    status_rule=
    expect_statuses=
    expect_oracle_requests=
    expect_rust_requests=
    stdout_mode=json
    case_org=$mock_org
    case_pat=$mock_pat
    case_extra=()
    case_stdin=
    compare_files=()
    forbid_commands=()

    finish_case
}

# The commands a case forbids must not be invoked by either side's run. The shim
# installed by mock_run is the record: it exists only while the case runs, and only
# a call of that command writes `<command>.used` beside it — so the failure names
# the side and the command, which is the flow the case exists to prove does not
# start.
mock_forbidden_check() { # mock_forbidden_check <slug>
    local slug=$1 side command used

    for side in oracle rust; do
        for command in ${forbid_commands[@]+"${forbid_commands[@]}"}; do
            used=$work/shims/$slug.$side/used.$command
            if [[ -f $used ]]; then
                fail "the $side invocation started the flow this case forbids: '$command' was invoked"
            fi
        done
    done
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

# D25's query difference, mechanically: the frozen client glues
# `?api-version=7.1` onto a path that already carries a query — `get_raw/2`'s
# `?fileName=…`, or the current-only iterations list's inline `?$timeframe=current`
# — so its one pair's value swallows the version, where this build sends the two
# pairs. Splitting that pair reproduces this build's spelling; a case that sets
# `rest_norm=$d25_query_norm` still fails on any other request difference.
# The tail is what a value with a second `?api-version=` occurrence keeps:
# `$parts[1:] | join("?api-version=")` rather than `$parts[1]`, so the normaliser
# cannot silently drop it (W2's C10; unreachable in today's scenarios, hardened
# anyway — a normaliser that loses bytes it does not name is a blind spot).
# D22's `+` spelling, mechanically: the frozen `URI.encode/1` leaves `+` raw in a
# path segment and this build escapes it `%2B`, so unescaping it on both sides
# reproduces one spelling; a case that sets `rest_norm=$d22_plus_norm` still fails
# on any other request difference, and the escaped form itself is pinned by the
# integration tests.
d22_plus_norm='map(.path |= gsub("%2B"; "+"))'

d25_query_norm='map(.query |= ([.[] | if (index("?api-version=") != null)
    then (. | split("?api-version=")) as $parts
       | ($parts[0] | split("=")) as $kv
       | (($kv[0]) + "=" + ($kv[1:] | join("="))), ("api-version=" + ($parts[1:] | join("?api-version=")))
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

# The shared vocabulary the direction filters are written in: `qpair("p")`
# asserts a request carried exactly the pair `p` as sent (so `state=` cannot match
# `stateFilter=`), `any_path("n")`/`any_body("n")` that some request's path/body
# contains `n`. Filters that need more write raw jq over the log objects.
direction_jq='def qpairs: [.[] | .query | split("&")[] | select(. != "")];
    def qpair($pair): (qpairs | index($pair)) != null;
    def any_path($needle): any(.[]; .path | contains($needle));
    def any_body($needle): any(.[]; (.body // "") | contains($needle));'

# The request-direction assertion: each side's filter runs over that side's
# recorded requests, read as one array. It is asserted on every case that sets one,
# whether or not the two sides differ — the one check a case that would otherwise
# read MATCH can still fail on. A satisfied filter adds no note, so the adoption
# leaves an unchanged run's output unchanged; a failing one names the side.
mock_direction_check() {
    local slug=$1 side var filter file result

    for side in oracle rust; do
        var=expect_${side}_requests
        filter=${!var}
        [[ -z $filter ]] && continue

        if [[ $side == oracle ]]; then
            file=$work/$slug.elixir.requests
        else
            file=$work/$slug.rust.requests
        fi

        if result=$(jq -s -c -e "$direction_jq $filter" "$file" 2>&1); then
            continue
        fi

        fail "the $side requests do not satisfy the direction filter ($result): $filter"
    done
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

    # `banners set --message @<file>`: the oracle sends this path literally and
    # this build reads the file (D44), from the same absolute path on both sides.
    printf 'Message from a file.\n\n' >"$work/banner-message.txt"

    # `test-results publish --file`: both binaries run from their own temp cwd, so
    # the path is absolute and shared, and the bytes are the file the capture
    # uploaded. The `.bin` sibling is the non-UTF-8 file the oracle cannot encode,
    # and `nested/` carries the same bytes under a path whose basename is what the
    # query names.
    printf '<?xml version="1.0" encoding="UTF-8"?>\n<testsuites/>\n' >"$work/results.xml"
    mkdir -p "$work/nested"
    printf '<?xml version="1.0" encoding="UTF-8"?>\n<testsuites/>\n' >"$work/nested/results.xml"
    printf '\377\376\000binary\n' >"$work/results.bin"

    # ── projects ──

    mock_case projects-list "projects list" projects list --json

    rest_rule='D19: the frozen CLI sends state/top/skip where this build sends the intended stateFilter/$top/$skip'
    expect_oracle_requests='qpair("state=wellFormed") and qpair("top=1") and qpair("skip=0")'
    expect_rust_requests='qpair("stateFilter=wellFormed") and qpair("%24top=1") and qpair("%24skip=0")'
    mock_case projects-list-filters "projects list --state/--top/--skip" \
        projects list --state wellFormed --top 1 --skip 0 --json

    rest_rule='D19: present means sent — the empty state and the two zeros reach the wire on both sides, under the two spellings'
    expect_oracle_requests='qpair("state=") and qpair("top=0") and qpair("skip=0")'
    expect_rust_requests='qpair("stateFilter=") and qpair("%24top=0") and qpair("%24skip=0")'
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
    # The WIQL body is what the case discriminates on, but the rule above absorbs
    # any other request difference, so the filters also name the two requests the
    # chain must send — the POST's path and api-version, the batch GET's path,
    # api-version and ids — and the count.
    expect_oracle_requests="length == 2 and any_body(\"FROM WorkItems WHERE AND [System.State] = 'Active'\") and any(.[]; .method == \"POST\" and .path == \"/ado-harness/Alpha/_apis/wit/wiql\" and (.query | split(\"&\") | index(\"api-version=7.1\")) != null) and any(.[]; .method == \"GET\" and .path == \"/ado-harness/_apis/wit/workitems\" and (.query | split(\"&\") | index(\"api-version=7.1\")) != null and (.query | split(\"&\") | index(\"ids=42%2C43\")) != null)"
    expect_rust_requests="length == 2 and any_body(\"FROM WorkItems WHERE [System.TeamProject] = 'Alpha' AND [System.WorkItemType] = 'Bug'\") and (any_body(\"AND AND\") | not) and any(.[]; .method == \"POST\" and .path == \"/ado-harness/Alpha/_apis/wit/wiql\" and (.query | split(\"&\") | index(\"api-version=7.1\")) != null) and any(.[]; .method == \"GET\" and .path == \"/ado-harness/_apis/wit/workitems\" and (.query | split(\"&\") | index(\"api-version=7.1\")) != null and (.query | split(\"&\") | index(\"ids=42%2C43\")) != null)"
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
    expect_oracle_requests='any(.[]; (.path | contains("/ado-harness/http")) and (.path | endswith("/blob/drop.zip")))'
    expect_rust_requests='any(.[]; .path == "/blob/drop.zip" and .query == "")'
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

    # Reshaped by Task 10 because the method-less form became the *browser* flow
    # on both sides (D29); named back by Task 11's Ruling B7, because a
    # set-but-blank `ADO_PAT` with no `--method` is a loud refusal here now — the
    # invocation is non-interactive on both sides again. The oracle treats `""`
    # as a value, infers `pat` from it, stores an empty token and exits 0; this
    # build refuses before any flow starts, stores nothing and sends nothing.
    #
    # This build's non-interactivity rests on that refusal, so `open` and
    # `xdg-open` are forbidden and shadowed for this case: if the refusal ever
    # regresses, the browser flow reaches the shim instead of a real opener and
    # the case fails by name. The residual is the wait, not the flow — the shim
    # still leaves the flow's 120 s accept timeout to run, and the harness has no
    # per-case timeout (§10). The oracle side is non-interactive for its own
    # reason (a blank value is a value to it, so no flow starts there either).
    envelope_rule='D56: a set-but-blank ADO_PAT with no --method is a loud validation error here (exit 1, nothing stored); the oracle treats "" as a PAT, stores an empty token and exits 0'
    status_rule='D56 (D16): a set-but-blank ADO_PAT reads as unset here (exit 1) where the frozen CLI treats it as a value, stores an empty token and exits 0'
    expect_statuses='0 1'
    expect_oracle_requests='length == 0'
    expect_rust_requests='length == 0'
    case_pat=
    case_extra=("ADO_PAT=")
    forbid_commands=(open xdg-open)
    mock_case login-blank-env-pat "login with a blank ADO_PAT (no --method)" \
        login --org "$mock_org" --json

    envelope_rule='D27c: the message says what was removed instead of the legacy ~/.ado_cli/config.json path'
    mock_case logout "logout" logout --json

    # Wave 3's browser method closed D29: both sides now name the same three methods
    # and refuse an unknown spelling with the same message. The refusal is decided
    # before any flow starts, so no request goes out — asserted on both sides. The
    # browser flow itself is deliberately absent: the oracle would open a real
    # browser and hold its accept for 120 s (spec §10).
    expect_oracle_requests='length == 0'
    expect_rust_requests='length == 0'
    mock_case login-unknown-method "login --method bogus" \
        login --method bogus --org "$mock_org" --json

    expect_oracle_requests='length == 0'
    expect_rust_requests='length == 0'
    mock_case login-blank-method "login --method ''" \
        login --method "" --org "$mock_org" --json

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

    status_rule='D5/D23 (R4): a required option the oracle never validates is a silent exit 0 there; this build makes it a loud usage error'
    expect_statuses='0 1'
    stdout_mode=text
    mock_case pipelines-folders-create-no-path "pipelines-folders create (no --path)" \
        pipelines-folders create Folders --json

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
    expect_oracle_requests='qpair("fileName=out.bin?api-version=7.1")'
    expect_rust_requests='qpair("fileName=out.bin")'
    envelope_rule='§8: the frozen download prints its success line plus the halt_success "Done." marker; this build prints the line alone'
    stdout_mode=text
    compare_files=(out.bin)
    mock_case workitems-attachments-download-output "workitems attachments download" \
        workitems attachments download 42 att-1 --output out.bin

    rest_rule='D25: the frozen get_raw appends api-version onto a path that already carries ?fileName=… (one query pair whose value swallows the version); this build sends fileName and api-version as separate pairs'
    rest_norm=$d25_query_norm
    expect_oracle_requests='qpair("fileName=out.bin?api-version=7.1")'
    expect_rust_requests='qpair("fileName=out.bin")'
    envelope_rule='§8: the frozen download prints its success line plus the halt_success "Done." marker; this build prints the line alone'
    stdout_mode=text
    compare_files=(out.bin)
    mock_case workitems-attachments-download-output-json "workitems attachments download (--json)" \
        workitems attachments download 42 att-1 --output out.bin --json

    rest_rule='D25: the frozen get_raw appends api-version onto a path that already carries ?fileName=… (one query pair whose value swallows the version); this build sends fileName and api-version as separate pairs'
    rest_norm=$d25_query_norm
    expect_oracle_requests='qpair("fileName=notes.txt?api-version=7.1")'
    expect_rust_requests='qpair("fileName=notes.txt")'
    envelope_rule='§8: the frozen download prints its success line plus the halt_success "Done." marker; this build prints the line alone'
    stdout_mode=text
    compare_files=(notes.txt)
    mock_case workitems-attachments-download-default "workitems attachments download (default name)" \
        workitems attachments download 42 att-2

    rest_rule='D25: the frozen get_raw appends api-version onto a path that already carries ?fileName=… (one query pair whose value swallows the version); this build sends fileName and api-version as separate pairs'
    rest_norm=$d25_query_norm
    expect_oracle_requests='qpair("fileName=attachment_att-3?api-version=7.1")'
    expect_rust_requests='qpair("fileName=attachment_att-3")'
    envelope_rule='§8: the frozen download prints its success line plus the halt_success "Done." marker; this build prints the line alone'
    stdout_mode=text
    compare_files=(attachment_att-3)
    mock_case workitems-attachments-download-no-name "workitems attachments download (no attributes.name)" \
        workitems attachments download 42 att-3

    case_stdin=$'n\n'
    rest_rule='D25: the frozen get_raw appends api-version onto a path that already carries ?fileName=… (one query pair whose value swallows the version); this build sends fileName and api-version as separate pairs'
    rest_norm=$d25_query_norm
    expect_oracle_requests='qpair("fileName=out.bin?api-version=7.1")'
    expect_rust_requests='qpair("fileName=out.bin")'
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
    expect_oracle_requests='length == 0'
    expect_rust_requests='any(.[]; (.path | endswith("/pullrequests")) and ((.body | fromjson | has("description")) | not))'
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
    expect_oracle_requests='any_path("/reviewers/ada@example.com")'
    expect_rust_requests='any_path("/reviewers/ada%40example.com")'
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

    # ── Wave 2: the area paths and the team iterations (Task 12) ──
    #
    # Captured shapes: areas hang off `wit/classificationNodes/areas`; the whole
    # area path is one segment (`Alpha\Team` → `Alpha%5CTeam`) but the frozen
    # `URI.encode/1` leaves `/` alone, so a slash in the path changes the URL's
    # structure (D22 — the `areas show` slash case). `list` and `show` emit the
    # value envelope; the writes' `--json` output is this build's value/message
    # envelope where the oracle prints its human line in both modes (D33), while
    # `areas show`'s 404 is the module's `Area path '…' not found` on stderr with
    # no envelope (D4). `--depth` is sent as `$depth`, `[--current]` as
    # `$timeframe` (D25's second site: the frozen path glue swallows the version).
    # The two areas' list error cases are carried here (C2): 404 and 500, both
    # D24's body rendering. `--start_date`/`--finish_date` are schema names the
    # frozen parser rejects; `--start-date`/`--finish-date` parse — and then crash
    # the command (D39, the `put_in/3` on a nil parent), which this build repairs.

    mock_case areas-list "areas list" \
        areas list Alpha --json

    mock_case areas-list-depth "areas list --depth 2" \
        areas list Alpha --depth 2 --json

    stdout_mode=text
    mock_case areas-list-human "areas list (human)" \
        areas list Alpha

    stdout_mode=text
    mock_case areas-list-empty-human "areas list (empty children, human)" \
        areas list Empty

    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with inspect/2 (C2: the list-error row)'
    mock_case areas-list-404 "areas list (404)" \
        areas list Missing --json

    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with inspect/2 (C2: the list-error row)'
    mock_case areas-list-500 "areas list (500)" \
        areas list Broken --json

    mock_case areas-show "areas show" \
        areas show Alpha 'Alpha\Team' --json

    stdout_mode=text
    mock_case areas-show-human "areas show (human)" \
        areas show Alpha 'Alpha\Team'

    rest_rule='D22: the whole area path is one path segment here, percent-encoded more strictly than the frozen URI.encode/1 (which left the / alone), so the request paths differ and the envelopes do not'
    expect_oracle_requests='any_path("/classificationNodes/areas/Alpha/Team")'
    expect_rust_requests='any_path("/classificationNodes/areas/Alpha%2FTeam")'
    mock_case areas-show-slash "areas show (slash in the path)" \
        areas show Alpha 'Alpha/Team' --json

    envelope_rule='D4: the frozen CLI writes the guard to stderr with no envelope under --json where this build emits the error envelope'
    mock_case areas-show-404 "areas show (404)" \
        areas show Alpha 'Alpha\Missing' --json

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the value envelope'
    mock_case areas-create "areas create" \
        areas create Alpha --name Team --json

    stdout_mode=text
    mock_case areas-create-human "areas create (human)" \
        areas create Alpha --name Team

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the value envelope'
    mock_case areas-create-parent "areas create --parent" \
        areas create Alpha --name Nested --parent 'Alpha\Team' --json

    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with inspect/2'
    mock_case areas-create-409 "areas create (409)" \
        areas create Conflict --name Duplicate --json

    status_rule='D34: a missing required option is a silent exit 0 in the oracle; this build is a loud usage error'
    expect_statuses='0 1'
    stdout_mode=text
    mock_case areas-create-no-name "areas create (no --name)" \
        areas create Alpha --json

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the value envelope'
    mock_case areas-update "areas update" \
        areas update Alpha 'Alpha\Team' --name Renamed --json

    stdout_mode=text
    mock_case areas-update-human "areas update (human)" \
        areas update Alpha 'Alpha\Team' --name Renamed

    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with inspect/2'
    mock_case areas-update-404 "areas update (404)" \
        areas update Alpha 'Alpha\Missing' --name Renamed --json

    status_rule='D34: a missing required option is a silent exit 0 in the oracle; this build is a loud usage error'
    expect_statuses='0 1'
    stdout_mode=text
    mock_case areas-update-no-name "areas update (no --name)" \
        areas update Alpha 'Alpha\Team' --json

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the message envelope'
    mock_case areas-delete "areas delete" \
        areas delete Alpha 'Alpha\Old' --json

    stdout_mode=text
    mock_case areas-delete-human "areas delete (human)" \
        areas delete Alpha 'Alpha\Old'

    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with inspect/2'
    mock_case areas-delete-404 "areas delete (404)" \
        areas delete Alpha 'Alpha\Missing' --json

    envelope_rule='D5: the oracle prints the command help on stdout before its usage error; this build writes clap’s message to stderr alone'
    stdout_mode=text
    mock_case areas-list-no-project "areas list (no project)" \
        areas list --json

    # ── iterations ──

    mock_case iterations-list "iterations list" \
        iterations list Alpha Team --json

    rest_rule='D25 (second site): the frozen list path carries an inline ?$timeframe=current and build_url appends ?api-version=7.1, so its one query pair swallows the version; this build sends $timeframe and api-version as separate pairs'
    rest_norm=$d25_query_norm
    expect_oracle_requests='qpair("$timeframe=current?api-version=7.1")'
    expect_rust_requests='qpair("%24timeframe=current")'
    mock_case iterations-list-current "iterations list --current" \
        iterations list Alpha Team --current --json

    stdout_mode=text
    mock_case iterations-list-empty-human "iterations list (empty, human)" \
        iterations list Empty Team

    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with inspect/2 (C2: the list-error row)'
    mock_case iterations-list-404 "iterations list (404)" \
        iterations list Missing Team --json

    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with inspect/2 (C2: the list-error row)'
    mock_case iterations-list-500 "iterations list (500)" \
        iterations list Broken Team --json

    mock_case iterations-show "iterations show" \
        iterations show Alpha Team aaaaaaaa-0001-0001-0001-000000000001 --json

    stdout_mode=text
    mock_case iterations-show-human "iterations show (human)" \
        iterations show Alpha Team aaaaaaaa-0001-0001-0001-000000000001

    envelope_rule='D4: the frozen CLI writes the guard to stderr with no envelope under --json where this build emits the error envelope'
    mock_case iterations-show-404 "iterations show (404)" \
        iterations show Alpha Team missing-id --json

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the value envelope'
    mock_case iterations-create "iterations create" \
        iterations create Alpha Team --name 'Sprint 24' --json

    stdout_mode=text
    mock_case iterations-create-human "iterations create (human)" \
        iterations create Alpha Team --name 'Sprint 24'

    status_rule='D39: the frozen body builder raises ArgumentError before sending (put_in on a nil attributes parent) and CLI.run/1 catches it into exit 1 with both streams empty; this build sends the intended attributes body'
    expect_statuses='1 0'
    rest_rule='D39: the oracle never sends the dated create (the put_in/3 crash); this build sends the intended {"name": …, "attributes": {"startDate": …, "finishDate": …}} body'
    envelope_rule='D39: the oracle crashes before any output; this build emits the created iteration under the value envelope'
    mock_case iterations-create-dates "iterations create --start-date --finish-date (D39)" \
        iterations create Dated Team --name 'Sprint 26' --start-date 2026-03-01 --finish-date 2026-03-14 --json

    status_rule='D34: a missing required option is a silent exit 0 in the oracle; this build is a loud usage error'
    expect_statuses='0 1'
    stdout_mode=text
    mock_case iterations-create-no-name "iterations create (no --name)" \
        iterations create Alpha Team --json

    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with inspect/2'
    mock_case iterations-create-409 "iterations create (409)" \
        iterations create Conflict Team --name Duplicate --json

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the value envelope'
    mock_case iterations-update "iterations update" \
        iterations update Alpha Team aaaaaaaa-0001-0001-0001-000000000001 --name 'Sprint 24 renamed' --json

    stdout_mode=text
    mock_case iterations-update-human "iterations update (human)" \
        iterations update Alpha Team aaaaaaaa-0001-0001-0001-000000000001 --name 'Sprint 24 renamed'

    status_rule='D39: the frozen body builder raises ArgumentError before sending (put_in on a nil attributes parent) and CLI.run/1 catches it into exit 1 with both streams empty; this build sends the intended attributes body'
    expect_statuses='1 0'
    rest_rule='D39: the oracle never sends the dated update (the put_in/3 crash); this build sends the intended {"attributes": {"startDate": …, "finishDate": …}} body'
    envelope_rule='D39: the oracle crashes before any output; this build emits the updated iteration under the value envelope'
    mock_case iterations-update-dates "iterations update --start-date --finish-date (D39)" \
        iterations update Dated Team dated-id --start-date 2026-03-01 --finish-date 2026-03-14 --json

    envelope_rule='D4: the frozen CLI writes the guard to stderr with no envelope under --json where this build emits the error envelope'
    mock_case iterations-update-no-options "iterations update (no options)" \
        iterations update Alpha Team aaaaaaaa-0001-0001-0001-000000000001 --json

    envelope_rule='D4: the frozen CLI writes the guard to stderr with no envelope under --json where this build emits the error envelope'
    mock_case iterations-update-404 "iterations update (404)" \
        iterations update Alpha Team missing-id --name Renamed --json

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the message envelope'
    mock_case iterations-delete "iterations delete" \
        iterations delete Alpha Team bbbbbbbb-0002-0002-0002-000000000002 --json

    stdout_mode=text
    mock_case iterations-delete-human "iterations delete (human)" \
        iterations delete Alpha Team bbbbbbbb-0002-0002-0002-000000000002

    envelope_rule='D4: the frozen CLI writes the guard to stderr with no envelope under --json where this build emits the error envelope'
    mock_case iterations-delete-404 "iterations delete (404)" \
        iterations delete Alpha Team missing-id --json

    envelope_rule='D5: the oracle prints the command help on stdout before its usage error; this build writes clap’s message to stderr alone'
    stdout_mode=text
    mock_case iterations-list-no-team "iterations list (no team)" \
        iterations list Alpha --json

    # ── Wave 2: the teams and the user entitlements (Task 13) ──
    #
    # Captured shapes: teams hang off `/{project}/_apis/teams`, with the members
    # leaf one level deeper (`…/teams/{team_id}/members`); `users` is
    # organization-scoped — every path is `/_apis/userentitlements`, no project
    # segment — so its list 404/500 rows are selected by `case_org`
    # (missing/broken): one path cannot carry two responses. Both lists and every
    # read emit the value envelope and MATCH. The writes' `--json` output is this
    # build's value/message envelope where the oracle prints its human line (D33);
    # `teams show|update|delete`'s and `users show|remove`'s 404s carry the
    # module's own wording in the envelope where the oracle writes it to stderr
    # with no envelope (D4), as does `teams update`'s no-option guard; the four
    # list error cases and the two 409s keep the classified envelope with this
    # build's raw error body where the oracle re-renders the decoded map (D24 —
    # C2's rows for both areas). `teams delete` never prompts (R1/R5): its
    # `(stdin n — no prompt)` case sends its request. `users remove` gained the
    # confirmation its own doc promises in Wave 3's rulings round (Ruling A1, D52):
    # it prompts unless `--force`, so its former `(stdin n — no prompt)` case is
    # now the `(stdin n)` refusal — the frozen CLI never asks, so **no users-remove
    # case may use the prompt modes**, which assert the oracle's `[y/N]` on stdout
    # (it writes none). The success path scripts a `y` to get past this side's
    # gate, and `--force` is the unknown flag the frozen parser refuses (D52's
    # second half, the case the tree had nowhere before). A missing
    # `--name`/`--email` is D34's silent exit 0 in the oracle. An email id is a
    # path segment and differs by the stricter encoding (D22 — both spellings
    # have their own route carrying the same body, so only the path differs). The
    # module docs promise a `--search` on `users list` that the frozen parser
    # rejects; it is not in this tree.

    mock_case teams-list "teams list" \
        teams list Alpha --json

    mock_case teams-list-top "teams list --top 5" \
        teams list Alpha --top 5 --json

    stdout_mode=text
    mock_case teams-list-empty-human "teams list (empty, human)" \
        teams list Empty

    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with inspect/2 (C2: the list-error row)'
    mock_case teams-list-404 "teams list (404)" \
        teams list Missing --json

    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with inspect/2 (C2: the list-error row)'
    mock_case teams-list-500 "teams list (500)" \
        teams list Broken --json

    envelope_rule='D5: the oracle prints the command help on stdout before its usage error; this build writes clap’s message to stderr alone'
    stdout_mode=text
    mock_case teams-list-no-project "teams list (no project)" \
        teams list

    mock_case teams-show "teams show" \
        teams show Alpha team-1 --json

    stdout_mode=text
    mock_case teams-show-human "teams show (human)" \
        teams show Alpha team-1

    rest_rule='D22: the email id is a path segment here, percent-encoded more strictly than the frozen URI.encode/1 (which left the @ alone), so the request paths differ and the envelopes do not'
    rest_norm='map(.path |= sub("%40"; "@"))'
    expect_oracle_requests='any_path("/teams/ada@example.com")'
    expect_rust_requests='any_path("/teams/ada%40example.com")'
    mock_case teams-show-email "teams show (email id)" \
        teams show Alpha ada@example.com --json

    envelope_rule='D4: the frozen CLI writes the guard to stderr with no envelope under --json where this build emits the error envelope'
    mock_case teams-show-404 "teams show (404)" \
        teams show Alpha missing-id --json

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the value envelope'
    mock_case teams-create "teams create" \
        teams create Alpha --name Team --json

    stdout_mode=text
    mock_case teams-create-human "teams create (human)" \
        teams create Alpha --name Team

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the value envelope'
    mock_case teams-create-description "teams create --description" \
        teams create Alpha2 --name Beta --description 'A team' --json

    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with inspect/2'
    mock_case teams-create-409 "teams create (409)" \
        teams create Conflict --name Duplicate --json

    status_rule='D34: a missing required option is a silent exit 0 in the oracle; this build is a loud usage error'
    expect_statuses='0 1'
    stdout_mode=text
    mock_case teams-create-no-name "teams create (no --name)" \
        teams create Alpha --json

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the value envelope'
    mock_case teams-update "teams update" \
        teams update Alpha team-1 --name Renamed --json

    stdout_mode=text
    mock_case teams-update-human "teams update (human)" \
        teams update Alpha team-1 --name Renamed

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the value envelope'
    mock_case teams-update-description "teams update --description" \
        teams update Alpha team-2 --description 'New desc' --json

    envelope_rule='D4: the frozen CLI writes the guard to stderr with no envelope under --json where this build emits the error envelope'
    mock_case teams-update-no-options "teams update (no options)" \
        teams update Alpha team-1 --json

    envelope_rule='D4: the frozen CLI writes the guard to stderr with no envelope under --json where this build emits the error envelope'
    mock_case teams-update-404 "teams update (404)" \
        teams update Alpha missing-id --name Renamed --json

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the message envelope'
    mock_case teams-delete "teams delete" \
        teams delete Alpha team-1 --json

    stdout_mode=text
    mock_case teams-delete-human "teams delete (human)" \
        teams delete Alpha team-1

    case_stdin=$'n\n'
    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the message envelope'
    mock_case teams-delete-stdin-n "teams delete (stdin n — no prompt)" \
        teams delete Alpha team-1 --json

    envelope_rule='D4: the frozen CLI writes the guard to stderr with no envelope under --json where this build emits the error envelope'
    mock_case teams-delete-404 "teams delete (404)" \
        teams delete Alpha missing-id --json

    mock_case teams-members-list "teams members list" \
        teams members list Alpha team-1 --json

    stdout_mode=text
    mock_case teams-members-list-empty-human "teams members list (empty, human)" \
        teams members list Alpha empty-team

    mock_case users-list "users list" \
        users list --json

    mock_case users-list-top "users list --top 5" \
        users list --top 5 --json

    case_org=empty-org
    stdout_mode=text
    mock_case users-list-empty-human "users list (empty org, human)" \
        users list

    case_org=missing
    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with inspect/2 (C2: the list-error row)'
    mock_case users-list-404 "users list (404)" \
        users list --json

    case_org=broken
    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with inspect/2 (C2: the list-error row)'
    mock_case users-list-500 "users list (500)" \
        users list --json

    envelope_rule='D5: the oracle prints the command help on stdout before its usage error; this build writes clap’s message to stderr alone'
    stdout_mode=text
    mock_case users-list-extra-positional "users list (extra positional)" \
        users list Alpha

    mock_case users-show "users show" \
        users show user-1 --json

    stdout_mode=text
    mock_case users-show-human "users show (human)" \
        users show user-1

    rest_rule='D22: the email id is a path segment here, percent-encoded more strictly than the frozen URI.encode/1 (which left the @ alone), so the request paths differ and the envelopes do not'
    rest_norm='map(.path |= sub("%40"; "@"))'
    expect_oracle_requests='any_path("/userentitlements/ada@example.com")'
    expect_rust_requests='any_path("/userentitlements/ada%40example.com")'
    mock_case users-show-email "users show (email id)" \
        users show ada@example.com --json

    envelope_rule='D4: the frozen CLI writes the guard to stderr with no envelope under --json where this build emits the error envelope'
    mock_case users-show-404 "users show (404)" \
        users show missing-id --json

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the value envelope'
    mock_case users-add "users add" \
        users add --email ada@example.com --json

    stdout_mode=text
    mock_case users-add-human "users add (human)" \
        users add --email ada@example.com

    case_org=user-org
    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the value envelope'
    mock_case users-add-license "users add --license" \
        users add --email grace@example.com --license stakeholder --json

    status_rule='D34: a missing required option is a silent exit 0 in the oracle; this build is a loud usage error'
    expect_statuses='0 1'
    stdout_mode=text
    mock_case users-add-no-email "users add (no --email)" \
        users add --json

    case_org=conflict-org
    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with inspect/2'
    mock_case users-add-409 "users add (409)" \
        users add --email ada@example.com --json

    # ── Ruling A1: `users remove`'s gate and its `--force` (D52) ──
    #
    # Captured: the frozen CLI sends its DELETE on EOF, on `n` and on `y` alike —
    # it never prompts — and rejects `--force` with `invalid option --force`, help
    # on stdout, exit 1, no request. This build asks the docstring's question on
    # stderr (D31), refuses on `n`/EOF with exit 1 and nothing sent (D32/D30), and
    # sends the DELETE with `--force` or a `y`. Each case below asserts the status
    # pair and both sides' requests, because the flipped cases' point is the
    # direction: the oracle proceeds where this build refuses, and the oracle
    # refuses the flag where this build proceeds.

    case_stdin=$'y\n'
    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the message envelope (the `y` gets past this side’s new gate, Ruling A1)'
    expect_oracle_requests='length == 1 and .[0].method == "DELETE" and .[0].path == "/ado-harness/_apis/userentitlements/user-1"'
    expect_rust_requests='length == 1 and .[0].method == "DELETE" and .[0].path == "/ado-harness/_apis/userentitlements/user-1"'
    stdout_mode=text
    mock_case users-remove "users remove (stdin y)" \
        users remove user-1 --json

    status_rule='D52 (D30): the frozen CLI never asks and removes on EOF; this build refuses with exit 1 and sends nothing'
    rest_rule='D52: the frozen CLI sends its DELETE on EOF; this build refuses without sending anything'
    expect_statuses='0 1'
    expect_oracle_requests='length == 1 and .[0].method == "DELETE" and .[0].path == "/ado-harness/_apis/userentitlements/user-1"'
    expect_rust_requests='length == 0'
    stdout_mode=text
    envelope_rule='D52: the frozen CLI prints its removed line (it never prompted); this build refuses on stderr with exit 1 and no document'
    mock_case users-remove-eof "users remove (EOF)" \
        users remove user-1 --json

    status_rule='D52 (D32): the frozen CLI ignores the `n` and removes; this build refuses with exit 1 and sends nothing'
    rest_rule='D52 (D32): the frozen CLI sends its DELETE despite the `n`; this build refuses without sending anything'
    expect_statuses='0 1'
    expect_oracle_requests='length == 1 and .[0].method == "DELETE" and .[0].path == "/ado-harness/_apis/userentitlements/user-1"'
    expect_rust_requests='length == 0'
    stdout_mode=text
    envelope_rule='D52 (D32): the frozen CLI removes despite the `n`; this build writes `Aborted.` to stderr with exit 1 and no document'
    case_stdin=$'n\n'
    mock_case users-remove-stdin-n "users remove (stdin n)" \
        users remove user-1 --json

    status_rule='D52 (D30): the frozen CLI never asks and removes on EOF; this build refuses with exit 1 and sends nothing'
    rest_rule='D52 (D30): the frozen CLI sends its DELETE on EOF; this build refuses without sending anything'
    expect_statuses='0 1'
    expect_oracle_requests='length == 1 and .[0].method == "DELETE" and .[0].path == "/ado-harness/_apis/userentitlements/user-1"'
    expect_rust_requests='length == 0'
    stdout_mode=text
    envelope_rule='D52: the frozen CLI prints its removed line; this build refuses on stderr with exit 1 and no document'
    mock_case users-remove-human "users remove (human, EOF)" \
        users remove user-1

    status_rule='D52 (D5): the frozen parser rejects --force as an unknown flag; this build accepts it and skips the question'
    rest_rule='D52 (D5): the frozen parser rejects --force and sends nothing; this build skips the question and sends the DELETE'
    expect_statuses='1 0'
    expect_oracle_requests='length == 0'
    expect_rust_requests='length == 1 and .[0].method == "DELETE" and .[0].path == "/ado-harness/_apis/userentitlements/user-1"'
    stdout_mode=text
    envelope_rule='D5: the oracle prints the command help on stdout before its usage error; this build writes the message envelope (D33) with no prompt'
    mock_case users-remove-force "users remove --force (the frozen parser refuses the flag)" \
        users remove user-1 --force --json

    case_stdin=$'y\n'
    envelope_rule='D4: the frozen CLI writes the guard to stderr with no envelope under --json where this build emits the error envelope'
    expect_oracle_requests='length == 1 and .[0].method == "DELETE" and .[0].path == "/ado-harness/_apis/userentitlements/missing-id"'
    expect_rust_requests='length == 1 and .[0].method == "DELETE" and .[0].path == "/ado-harness/_apis/userentitlements/missing-id"'
    mock_case users-remove-404 "users remove (404, stdin y)" \
        users remove missing-id --json

    # ── Wave 2: the branch policies (Task 14) ──
    #
    # `branch-policies` is the runnable spelling of the schema's `ado repos
    # policies` (R3/D18): the display name is rejected by both parsers. The REST
    # surface is `/{project}/_apis/policy/configurations`, where the frozen list
    # glues `?repositoryId=…` into the path and then appends `?api-version=7.1`,
    # so its repositoryId value swallows the version — D25's third site, and every
    # list case carries the normaliser as its `rest_norm` because of it. The two
    # writes' `--json` output is this build's value/message envelope where the
    # oracle prints its human line (D33); `show`/`update`'s GET 404 is the module's
    # own wording on stderr there and an envelope here (D4); the PUT 404/409 and
    # the 409 on create keep the classified envelope with this build's raw error
    # body where the oracle renders the decoded map (D24); the delete 404 MATCHes
    # because the frozen `Client.delete/2` passes its body on undecoded, which is
    # exactly what this build's raw-body envelope carries. `--blocking`/
    # `--no-blocking` (and `--enabled`/`--no-enabled`) are one flag pair here where
    # the oracle's OptionParser `--no-` prefix does the same; both sides take the
    # last spelling given. A missing `--type`/`--branch` is D34's silent exit 0 in
    # the oracle. `delete` never prompts (R1/R5): the `(stdin n — no prompt)` and
    # `(EOF)` cases send their DELETEs.

    rest_rule='D25: the frozen list glues `?repositoryId=…` onto the path and then appends `?api-version=7.1`, so its one pair swallows the version; this build sends the two pairs'
    rest_norm=$d25_query_norm
    expect_oracle_requests='qpair("repositoryId=Alpha.Core?api-version=7.1")'
    expect_rust_requests='qpair("repositoryId=Alpha.Core")'
    mock_case policies-list "branch-policies list" \
        branch-policies list Alpha Alpha.Core --json

    rest_rule='D25: the frozen list glues `?repositoryId=…` onto the path and then appends `?api-version=7.1`; this build sends repositoryId, branch and api-version as three pairs'
    rest_norm=$d25_query_norm
    expect_oracle_requests='qpair("repositoryId=Alpha.Core?api-version=7.1") and qpair("branch=main")'
    expect_rust_requests='qpair("repositoryId=Alpha.Core") and qpair("branch=main")'
    mock_case policies-list-branch "branch-policies list --branch" \
        branch-policies list Alpha Alpha.Core --branch main --json

    rest_rule='D25: the frozen list glues `?repositoryId=…` onto the path and then appends `?api-version=7.1`; this build sends the two pairs'
    rest_norm=$d25_query_norm
    expect_oracle_requests='qpair("repositoryId=Alpha.Core?api-version=7.1")'
    expect_rust_requests='qpair("repositoryId=Alpha.Core")'
    stdout_mode=text
    mock_case policies-list-empty-human "branch-policies list (empty, human)" \
        branch-policies list Empty Alpha.Core

    rest_rule='D25: the frozen list glues `?repositoryId=…` onto the path and then appends `?api-version=7.1`; this build sends the two pairs'
    rest_norm=$d25_query_norm
    expect_oracle_requests='qpair("repositoryId=Alpha.Core?api-version=7.1")'
    expect_rust_requests='qpair("repositoryId=Alpha.Core")'
    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with inspect/2 (C2: the list-error row)'
    mock_case policies-list-404 "branch-policies list (404)" \
        branch-policies list Missing Alpha.Core --json

    rest_rule='D25: the frozen list glues `?repositoryId=…` onto the path and then appends `?api-version=7.1`; this build sends the two pairs'
    rest_norm=$d25_query_norm
    expect_oracle_requests='qpair("repositoryId=Alpha.Core?api-version=7.1")'
    expect_rust_requests='qpair("repositoryId=Alpha.Core")'
    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with inspect/2 (C2: the list-error row)'
    mock_case policies-list-500 "branch-policies list (500)" \
        branch-policies list Broken Alpha.Core --json

    mock_case policies-show "branch-policies show" \
        branch-policies show Alpha Alpha.Core 42 --json

    stdout_mode=text
    mock_case policies-show-human "branch-policies show (human)" \
        branch-policies show Alpha Alpha.Core 42

    mock_case policies-show-reviewers "branch-policies show (second type)" \
        branch-policies show Alpha Alpha.Core 43 --json

    mock_case policies-show-bare "branch-policies show (bare policy)" \
        branch-policies show Alpha Alpha.Core 52 --json

    envelope_rule='D4: the frozen CLI writes the guard to stderr with no envelope under --json where this build emits the error envelope'
    mock_case policies-show-404 "branch-policies show (404)" \
        branch-policies show Alpha Alpha.Core 999 --json

    envelope_rule='D5: the oracle prints the command help on stdout before its usage error; this build writes clap’s message to stderr alone'
    stdout_mode=text
    mock_case policies-show-non-integer "branch-policies show (non-integer id)" \
        branch-policies show Alpha Alpha.Core not-an-integer --json

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the value envelope'
    mock_case policies-create "branch-policies create" \
        branch-policies create Alpha Alpha.Core \
        --type fa4e907d-c16b-4a4c-9dfa-4906e5d171dd --branch refs/heads/main --json

    stdout_mode=text
    mock_case policies-create-human "branch-policies create (human)" \
        branch-policies create Alpha Alpha.Core \
        --type fa4e907d-c16b-4a4c-9dfa-4906e5d171dd --branch refs/heads/main

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the value envelope'
    mock_case policies-create-reviewers "branch-policies create --no-blocking (second type)" \
        branch-policies create Alpha2 Beta.Core \
        --type fd2167ab-9d2a-4d8b-b2c9-1cdfbb6d4c34 --branch refs/heads/release/2.0 \
        --no-blocking --json

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the value envelope'
    mock_case policies-create-both-flags "branch-policies create --blocking --no-blocking (last wins)" \
        branch-policies create Both Alpha.Core \
        --type fa4e907d-c16b-4a4c-9dfa-4906e5d171dd --branch refs/heads/main \
        --blocking --no-blocking --json

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the value envelope'
    mock_case policies-create-reverse "branch-policies create --no-blocking --blocking (last wins)" \
        branch-policies create Reverse Alpha.Core \
        --type fa4e907d-c16b-4a4c-9dfa-4906e5d171dd --branch refs/heads/main \
        --no-blocking --blocking --json

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the value envelope'
    mock_case policies-create-wildcard "branch-policies create (wildcard ref)" \
        branch-policies create Wildcard Wild \
        --type 0609b952-1397-4640-95ec-e121a052fb4b --branch 'refs/heads/feature/*' --json

    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with inspect/2'
    mock_case policies-create-409 "branch-policies create (409)" \
        branch-policies create Conflict Alpha.Core \
        --type fa4e907d-c16b-4a4c-9dfa-4906e5d171dd --branch refs/heads/main --json

    status_rule='D34: a missing required option is a silent exit 0 in the oracle; this build is a loud usage error'
    expect_statuses='0 1'
    stdout_mode=text
    mock_case policies-create-no-type "branch-policies create (no --type)" \
        branch-policies create Alpha Alpha.Core --branch refs/heads/main --json

    status_rule='D34: a missing required option is a silent exit 0 in the oracle; this build is a loud usage error'
    expect_statuses='0 1'
    stdout_mode=text
    mock_case policies-create-no-branch "branch-policies create (no --branch)" \
        branch-policies create Alpha Alpha.Core \
        --type fa4e907d-c16b-4a4c-9dfa-4906e5d171dd --json

    envelope_rule='D5: the oracle prints the command help on stdout before its usage error; this build writes clap’s message to stderr alone'
    stdout_mode=text
    mock_case policies-create-unknown-flag "branch-policies create --type-id (unknown flag)" \
        branch-policies create Alpha Alpha.Core --type-id fa4e907d

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the value envelope'
    mock_case policies-update-blocking "branch-policies update --blocking" \
        branch-policies update Alpha Alpha.Core 44 --blocking --json

    stdout_mode=text
    mock_case policies-update-blocking-human "branch-policies update --blocking (human)" \
        branch-policies update Alpha Alpha.Core 44 --blocking

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the value envelope'
    mock_case policies-update-enabled "branch-policies update --no-enabled (second type)" \
        branch-policies update Alpha Alpha.Core 45 --no-enabled --json

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the value envelope'
    mock_case policies-update-both-flags "branch-policies update --no-blocking --no-enabled" \
        branch-policies update Alpha Alpha.Core 46 --no-blocking --no-enabled --json

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the value envelope'
    mock_case policies-update-no-options "branch-policies update (no options)" \
        branch-policies update Alpha Alpha.Core 47 --json

    stdout_mode=text
    mock_case policies-update-no-options-human "branch-policies update (no options, human)" \
        branch-policies update Alpha Alpha.Core 47

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the value envelope'
    mock_case policies-update-bare-policy "branch-policies update (a policy without type or flags)" \
        branch-policies update Alpha Alpha.Core 54 --json

    envelope_rule='D4: the frozen CLI writes the guard to stderr with no envelope under --json where this build emits the error envelope'
    mock_case policies-update-get-404 "branch-policies update (GET 404)" \
        branch-policies update Alpha Alpha.Core 998 --blocking --json

    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with inspect/2'
    mock_case policies-update-put-404 "branch-policies update (PUT 404)" \
        branch-policies update Alpha Alpha.Core 48 --no-enabled --json

    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with inspect/2'
    mock_case policies-update-put-409 "branch-policies update (PUT 409)" \
        branch-policies update Alpha Alpha.Core 49 --no-blocking --json

    envelope_rule='D5: the oracle prints the command help on stdout before its usage error; this build writes clap’s message to stderr alone'
    stdout_mode=text
    mock_case policies-update-non-integer "branch-policies update (non-integer id)" \
        branch-policies update Alpha Alpha.Core not-an-integer --blocking --json

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the message envelope'
    mock_case policies-delete "branch-policies delete" \
        branch-policies delete Alpha Alpha.Core 42 --json

    stdout_mode=text
    mock_case policies-delete-human "branch-policies delete (human)" \
        branch-policies delete Alpha Alpha.Core 42

    case_stdin=$'n\n'
    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the message envelope'
    mock_case policies-delete-stdin-n "branch-policies delete (stdin n — no prompt)" \
        branch-policies delete Alpha Alpha.Core 997 --json

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the message envelope'
    mock_case policies-delete-eof "branch-policies delete (EOF — no prompt)" \
        branch-policies delete Alpha Alpha.Core 997 --json

    mock_case policies-delete-404 "branch-policies delete (404)" \
        branch-policies delete Alpha Alpha.Core 999 --json

    envelope_rule='D5: the oracle prints the command help on stdout before its usage error; this build writes clap’s message to stderr alone'
    stdout_mode=text
    mock_case policies-delete-force "branch-policies delete --force (unknown flag)" \
        branch-policies delete Alpha Alpha.Core 42 --force

    envelope_rule='D5: the oracle prints the parent help on stdout before its unknown-subcommand line; this build writes clap’s message to stderr alone (R3: the display name is not runnable)'
    stdout_mode=text
    mock_case policies-display-name "repos policies list (the schema display name)" \
        repos policies list Alpha Alpha.Core

    # ── Wave 2: packages, releases and wikis (Task 15) ──
    #
    # The three groups' REST surfaces: `_apis/packaging/feeds/{feed}/packages`
    # (the three-positional area, `show` taking four), `_apis/release/releases`
    # (the classic-release surface with its three list filters) and
    # `_apis/wiki/wikis` with its `pages` grandchild. The package version
    # `1.0.0+build.5`, the package name `name+plus` and the wiki id `a+b` are the
    # D22 sites: the frozen `URI.encode/1` leaves `+` raw and this build escapes
    # it `%2B`, so both spellings have a route and the case carries the request
    # rule. `releases list --definition_id` is the schema's name, not the
    # runnable flag (D17's class — the probe rejects it, and the harness records
    # it). The four page commands' missing `--path`/`--content` are D34's
    # `Map.fetch!` crash (silent exit 0 there, a loud usage error here). The two
    # page writes are D33 under `--json`; `pages show` is D40 — the frozen read
    # path writes the page's content before its envelope and prints it twice in
    # human mode, where this build emits one document (or one copy).

    mock_case packages-list "packages list" \
        packages list Alpha feed-1 --json

    stdout_mode=text
    mock_case packages-list-empty-human "packages list (empty, human)" \
        packages list Alpha feed-empty

    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with inspect/2 (C2: the list-error row)'
    mock_case packages-list-404 "packages list (404)" \
        packages list Missing feed-1 --json

    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with inspect/2 (C2: the list-error row)'
    mock_case packages-list-500 "packages list (500)" \
        packages list Broken feed-1 --json

    mock_case packages-versions "packages versions" \
        packages versions Alpha feed-1 myapp-builds --json

    stdout_mode=text
    mock_case packages-versions-empty-human "packages versions (empty, human)" \
        packages versions Alpha feed-1 empty-pkg

    rest_rule='D22: the package name is a path segment here, percent-encoded more strictly than the frozen URI.encode/1 (which left the + alone), so the request paths differ and the envelopes do not'
    rest_norm=$d22_plus_norm
    expect_oracle_requests='any_path("/packages/name+plus/versions")'
    expect_rust_requests='any_path("/packages/name%2Bplus/versions")'
    mock_case packages-versions-plus "packages versions (a name with +)" \
        packages versions Alpha feed-1 name+plus --json

    mock_case packages-show "packages show" \
        packages show Alpha feed-1 myapp-builds 1.0.0 --json

    rest_rule='D22: the version is a path segment here, percent-encoded more strictly than the frozen URI.encode/1 (which left the + alone), so the request paths differ and the envelopes do not'
    rest_norm=$d22_plus_norm
    expect_oracle_requests='any_path("/versions/1.0.0+build.5")'
    expect_rust_requests='any_path("/versions/1.0.0%2Bbuild.5")'
    mock_case packages-show-plus "packages show (a version with +)" \
        packages show Alpha feed-1 myapp-builds 1.0.0+build.5 --json

    envelope_rule='D4: the frozen CLI writes the guard to stderr with no envelope under --json where this build emits the error envelope'
    mock_case packages-show-404 "packages show (404)" \
        packages show Alpha feed-1 myapp-builds 9.9.9 --json

    envelope_rule='D5: the oracle prints the command help on stdout before its usage error; this build writes clap’s message to stderr alone'
    stdout_mode=text
    mock_case packages-show-missing-version "packages show (no version)" \
        packages show Alpha feed-1 myapp-builds

    envelope_rule='D5: the oracle prints the command help on stdout before its usage error; this build writes clap’s message to stderr alone'
    stdout_mode=text
    mock_case packages-list-missing-feed "packages list (no feed id)" \
        packages list Alpha

    envelope_rule='D5: the oracle prints the command help on stdout before its usage error; this build writes clap’s message to stderr alone'
    stdout_mode=text
    mock_case packages-versions-missing-name "packages versions (no package name)" \
        packages versions Alpha feed-1

    mock_case releases-list "releases list" \
        releases list Alpha --json

    mock_case releases-list-flags "releases list --top --definition-id --status" \
        releases list Alpha --top 5 --definition-id 3 --status active --json

    stdout_mode=text
    mock_case releases-list-empty-human "releases list (empty, human)" \
        releases list Alpha --status none

    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with inspect/2 (C2: the list-error row)'
    mock_case releases-list-404 "releases list (404)" \
        releases list Missing --json

    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with inspect/2 (C2: the list-error row)'
    mock_case releases-list-500 "releases list (500)" \
        releases list Broken --json

    envelope_rule='D5: the oracle prints the command help on stdout before its usage error; this build writes clap’s message to stderr alone'
    stdout_mode=text
    mock_case releases-list-underscore-definition-id "releases list --definition_id (the schema name, not the flag)" \
        releases list Alpha --definition_id 3

    envelope_rule='D5: the oracle prints the command help on stdout before its usage error; this build writes clap’s message to stderr alone'
    stdout_mode=text
    mock_case releases-list-top-noninteger "releases list --top abc (not an integer)" \
        releases list Alpha --top abc

    mock_case releases-show "releases show" \
        releases show Alpha 101 --json

    stdout_mode=text
    mock_case releases-show-human "releases show (human)" \
        releases show Alpha 101

    stdout_mode=text
    mock_case releases-show-minimal-human "releases show (a release without definition/creator/environments)" \
        releases show Alpha 102

    envelope_rule='D4: the frozen CLI writes the guard to stderr with no envelope under --json where this build emits the error envelope'
    mock_case releases-show-404 "releases show (404)" \
        releases show Alpha 999 --json

    envelope_rule='D5: the oracle prints the command help on stdout before its usage error; this build writes clap’s message to stderr alone'
    stdout_mode=text
    mock_case releases-show-noninteger "releases show (non-integer id)" \
        releases show Alpha not-an-integer

    mock_case wikis-list "wikis list" \
        wikis list Alpha --json

    stdout_mode=text
    mock_case wikis-list-empty-human "wikis list (empty, human)" \
        wikis list Empty

    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with inspect/2 (C2: the list-error row)'
    mock_case wikis-list-404 "wikis list (404)" \
        wikis list Missing --json

    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with inspect/2 (C2: the list-error row)'
    mock_case wikis-list-500 "wikis list (500)" \
        wikis list Broken --json

    mock_case wikis-show "wikis show" \
        wikis show Alpha wiki-1 --json

    stdout_mode=text
    mock_case wikis-show-human "wikis show (human)" \
        wikis show Alpha wiki-1

    rest_rule='D22: the wiki id is a path segment here, percent-encoded more strictly than the frozen URI.encode/1 (which left the + alone), so the request paths differ and the envelopes do not'
    rest_norm=$d22_plus_norm
    expect_oracle_requests='any_path("/wikis/a+b")'
    expect_rust_requests='any_path("/wikis/a%2Bb")'
    mock_case wikis-show-plus "wikis show (a wiki id with +)" \
        wikis show Alpha a+b --json

    envelope_rule='D4: the frozen CLI writes the guard to stderr with no envelope under --json where this build emits the error envelope'
    mock_case wikis-show-404 "wikis show (404)" \
        wikis show Alpha nope --json

    mock_case pages-list "wikis pages list" \
        wikis pages list Alpha wiki-1 --json

    mock_case pages-list-path "wikis pages list --path" \
        wikis pages list Alpha wiki-1 --path /Design --json

    stdout_mode=text
    mock_case pages-list-empty-human "wikis pages list (empty, human)" \
        wikis pages list Alpha wiki-empty

    envelope_rule='D40: the frozen read path writes the page content in front of its envelope even under --json; this build emits the value envelope as the command’s only document'
    mock_case pages-show-json "wikis pages show (--json)" \
        wikis pages show Alpha wiki-1 --path /Home --json

    envelope_rule='D40: the frozen read path writes the page content twice in human mode (its unconditional writeln/1 ahead of json_or_format/3); this build writes it once'
    stdout_mode=text
    mock_case pages-show-human "wikis pages show (human)" \
        wikis pages show Alpha wiki-1 --path /Home

    envelope_rule='D4: the frozen CLI writes the guard to stderr with no envelope under --json where this build emits the error envelope'
    mock_case pages-show-404 "wikis pages show (404)" \
        wikis pages show Alpha wiki-1 --path /Missing --json

    status_rule='D34: a missing required option is a silent exit 0 in the oracle (the module’s Map.fetch! crash); this build is a loud usage error'
    expect_statuses='0 1'
    stdout_mode=text
    mock_case pages-show-no-path "wikis pages show (no --path)" \
        wikis pages show Alpha wiki-1

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the value envelope'
    mock_case pages-create "wikis pages create" \
        wikis pages create Alpha wiki-create --path /New-Page --content 'hello world' --json

    stdout_mode=text
    mock_case pages-create-human "wikis pages create (human)" \
        wikis pages create Alpha wiki-create --path /New-Page --content 'hello world'

    stdout_mode=text
    mock_case pages-create-multiword "wikis pages create (unquoted multiword --content)" \
        wikis pages create Alpha wiki-create --path /New-Page --content hello world

    status_rule='D34: a missing required option is a silent exit 0 in the oracle (the module’s Map.fetch! crash); this build is a loud usage error'
    expect_statuses='0 1'
    stdout_mode=text
    mock_case pages-create-no-path "wikis pages create (no --path)" \
        wikis pages create Alpha wiki-create --content hello

    status_rule='D34: a missing required option is a silent exit 0 in the oracle (the module’s Map.fetch! crash); this build is a loud usage error'
    expect_statuses='0 1'
    stdout_mode=text
    mock_case pages-create-no-content "wikis pages create (no --content)" \
        wikis pages create Alpha wiki-create --path /New-Page

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the value envelope'
    mock_case pages-update "wikis pages update" \
        wikis pages update Alpha wiki-update --path /Home --content 'new text' --json

    stdout_mode=text
    mock_case pages-update-human "wikis pages update (human)" \
        wikis pages update Alpha wiki-update --path /Home --content 'new text'

    stdout_mode=text
    mock_case pages-update-no-etag "wikis pages update (a read without an eTag)" \
        wikis pages update Alpha wiki-noetag --path /Home --content 'new text'

    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with inspect/2'
    mock_case pages-update-get-404 "wikis pages update (GET 404)" \
        wikis pages update Alpha wiki-gone --path /Home --content 'new text' --json

    status_rule='D34: a missing required option is a silent exit 0 in the oracle (the module’s Map.fetch! crash); this build is a loud usage error'
    expect_statuses='0 1'
    stdout_mode=text
    mock_case pages-update-no-path "wikis pages update (no --path)" \
        wikis pages update Alpha wiki-update --content new

    # ── Wave 3: the agent pools and the test coverage (Task 2) ──
    #
    # `agent-pools` and `test-coverage` are already the runnable spellings the
    # schema names, so no D17/D18 renaming. `agent-pools show` is the wave’s
    # first two-request read: the pool, then its agents; the `--json` result is
    # `{"pool": …, "agents": <the agents body>}` when the agents fetch
    # succeeds and the bare pool when it fails (any failure, still exit 0 — the
    # captured 500 route). Nothing in the two areas sends a spelling that
    # differs between the sides, so no case carries a `rest_rule`; each case
    # whose point is a request shape states it on both sides with the direction
    # filters instead.
    #
    # The oracle’s `agent-pools show` human detail is broken twice over: the
    # module wraps its payloads as an atom-keyed map while the formatter reads
    # the string keys `pool`/`agents` (so its four fields print empty), and
    # `print_agents_detail/2` needs a **list** `agents` member while the wrapped
    # value is the endpoint’s body map (so no agents print there either, where
    # this build prints the pool’s four fields). Wave 3’s Task 11 repairs the
    # second half (Ruling B3, D42): the human view unwraps `agents["value"]`, so
    # the success human case below is EXPECTED-DIFF — the frozen CLI’s four empty
    # fields and no agents beside this build’s pool fields and its agents block —
    # and the agents-fail human path still matches byte for byte (its map *is*
    # the pool). The integration suite pins this build’s fields, layout and
    # agents block. `test-coverage show`’s no-coverage branch is the read whose
    # frozen `--json` is prose (D21’s class): its human case matches, its json
    # case carries the rule.

    expect_oracle_requests='length == 1 and any_path("/ado-harness/_apis/distributedtask/pools") and qpair("api-version=7.1")'
    expect_rust_requests='length == 1 and any_path("/ado-harness/_apis/distributedtask/pools") and qpair("api-version=7.1")'
    mock_case agent-pools-list "agent-pools list" \
        agent-pools list --json

    case_org=empty-org
    stdout_mode=text
    mock_case agent-pools-list-empty-human "agent-pools list (empty org, human)" \
        agent-pools list

    case_org=missing
    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with inspect/2 (C2: the list-error row)'
    mock_case agent-pools-list-404 "agent-pools list (404)" \
        agent-pools list --json

    case_org=broken
    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with inspect/2 (C2: the list-error row)'
    mock_case agent-pools-list-500 "agent-pools list (500)" \
        agent-pools list --json

    expect_oracle_requests='[.[] | .path] == ["/ado-harness/_apis/distributedtask/pools/1", "/ado-harness/_apis/distributedtask/pools/1/agents"]'
    expect_rust_requests='[.[] | .path] == ["/ado-harness/_apis/distributedtask/pools/1", "/ado-harness/_apis/distributedtask/pools/1/agents"]'
    mock_case agent-pools-show "agent-pools show (the pool + its agents)" \
        agent-pools show 1 --json

    expect_oracle_requests='[.[] | .path] == ["/ado-harness/_apis/distributedtask/pools/1", "/ado-harness/_apis/distributedtask/pools/1/agents"]'
    expect_rust_requests='[.[] | .path] == ["/ado-harness/_apis/distributedtask/pools/1", "/ado-harness/_apis/distributedtask/pools/1/agents"]'
    stdout_mode=text
    envelope_rule='D42: the frozen view prints four empty fields and no agents (its atom-keyed wrapper and its map-shaped agents member); this build prints the pool’s fields and its agents block (Ruling B3)'
    mock_case agent-pools-show-human "agent-pools show (the pool + its agents, human)" \
        agent-pools show 1

    expect_oracle_requests='[.[] | .path] == ["/ado-harness/_apis/distributedtask/pools/2", "/ado-harness/_apis/distributedtask/pools/2/agents"]'
    expect_rust_requests='[.[] | .path] == ["/ado-harness/_apis/distributedtask/pools/2", "/ado-harness/_apis/distributedtask/pools/2/agents"]'
    mock_case agent-pools-show-agents-fail "agent-pools show (agents 500 — the bare pool)" \
        agent-pools show 2 --json

    expect_oracle_requests='[.[] | .path] == ["/ado-harness/_apis/distributedtask/pools/2", "/ado-harness/_apis/distributedtask/pools/2/agents"]'
    expect_rust_requests='[.[] | .path] == ["/ado-harness/_apis/distributedtask/pools/2", "/ado-harness/_apis/distributedtask/pools/2/agents"]'
    stdout_mode=text
    mock_case agent-pools-show-agents-fail-human "agent-pools show (agents 500, human)" \
        agent-pools show 2

    envelope_rule='D4: the frozen CLI writes the guard to stderr with no envelope under --json where this build emits the error envelope'
    expect_oracle_requests='[.[] | .path] == ["/ado-harness/_apis/distributedtask/pools/999"]'
    expect_rust_requests='[.[] | .path] == ["/ado-harness/_apis/distributedtask/pools/999"]'
    mock_case agent-pools-show-404 "agent-pools show (404)" \
        agent-pools show 999 --json

    envelope_rule='D5: the oracle prints the command help on stdout before its usage error; this build writes clap’s message to stderr alone'
    stdout_mode=text
    mock_case agent-pools-show-no-pool-id "agent-pools show (no pool_id)" \
        agent-pools show --json

    envelope_rule='D5: the oracle prints the command help on stdout before its usage error; this build writes clap’s message to stderr alone'
    stdout_mode=text
    mock_case agent-pools-show-noninteger "agent-pools show (non-integer pool_id)" \
        agent-pools show not-an-integer --json

    expect_oracle_requests='length == 1 and any_path("/ado-harness/Alpha/_apis/distributedtask/queues") and qpair("api-version=7.1")'
    expect_rust_requests='length == 1 and any_path("/ado-harness/Alpha/_apis/distributedtask/queues") and qpair("api-version=7.1")'
    mock_case agent-pools-queues-list "agent-pools queues list" \
        agent-pools queues list Alpha --json

    expect_oracle_requests='length == 1 and any_path("/ado-harness/Alpha/_apis/distributedtask/queues") and qpair("api-version=7.1") and qpair("poolId=1")'
    expect_rust_requests='length == 1 and any_path("/ado-harness/Alpha/_apis/distributedtask/queues") and qpair("api-version=7.1") and qpair("poolId=1")'
    mock_case agent-pools-queues-list-pool "agent-pools queues list --pool" \
        agent-pools queues list Alpha --pool 1 --json

    stdout_mode=text
    mock_case agent-pools-queues-list-empty-human "agent-pools queues list (empty, human)" \
        agent-pools queues list Empty

    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with inspect/2 (C2: the list-error row)'
    mock_case agent-pools-queues-list-404 "agent-pools queues list (404)" \
        agent-pools queues list Missing --json

    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with inspect/2 (C2: the list-error row)'
    mock_case agent-pools-queues-list-500 "agent-pools queues list (500)" \
        agent-pools queues list Broken --json

    envelope_rule='D5: the oracle prints the command help on stdout before its usage error; this build writes clap’s message to stderr alone'
    stdout_mode=text
    mock_case agent-pools-queues-list-pool-noninteger "agent-pools queues list --pool abc (not an integer)" \
        agent-pools queues list Alpha --pool abc --json

    envelope_rule='D5: the oracle prints the command help on stdout before its usage error; this build writes clap’s message to stderr alone'
    stdout_mode=text
    mock_case agent-pools-queues-list-no-project "agent-pools queues list (no project)" \
        agent-pools queues list --json

    expect_oracle_requests='length == 1 and any_path("/ado-harness/Alpha/_apis/test/codecoverage") and qpair("api-version=7.1") and qpair("buildId=42")'
    expect_rust_requests='length == 1 and any_path("/ado-harness/Alpha/_apis/test/codecoverage") and qpair("api-version=7.1") and qpair("buildId=42")'
    mock_case test-coverage-show "test-coverage show" \
        test-coverage show Alpha 42 --json

    stdout_mode=text
    mock_case test-coverage-show-human "test-coverage show (human)" \
        test-coverage show Alpha 42

    mock_case test-coverage-empty-json "test-coverage show (empty coverageData)" \
        test-coverage show Empty 42 --json

    stdout_mode=text
    mock_case test-coverage-empty-human "test-coverage show (empty coverageData, human)" \
        test-coverage show Empty 42

    envelope_rule='D21: the frozen no-coverage path prints its human sentence and Done. even under --json (show_no_coverage never reaches Output); this build emits the empty value envelope'
    mock_case test-coverage-nodata-json "test-coverage show (no coverageData, --json)" \
        test-coverage show NoCoverage 43 --json

    stdout_mode=text
    mock_case test-coverage-nodata-human "test-coverage show (no coverageData, human)" \
        test-coverage show NoCoverage 43

    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with inspect/2'
    mock_case test-coverage-404 "test-coverage show (404)" \
        test-coverage show Missing 42 --json

    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with inspect/2'
    mock_case test-coverage-500 "test-coverage show (500)" \
        test-coverage show Broken 42 --json

    envelope_rule='D5: the oracle prints the command help on stdout before its usage error; this build writes clap’s message to stderr alone'
    stdout_mode=text
    mock_case test-coverage-no-build-id "test-coverage show (no build_id)" \
        test-coverage show Alpha

    envelope_rule='D5: the oracle prints the command help on stdout before its usage error; this build writes clap’s message to stderr alone'
    stdout_mode=text
    mock_case test-coverage-noninteger "test-coverage show (non-integer build_id)" \
        test-coverage show Alpha not-an-integer

    # ── Wave 3: service connections (Task 3) ──
    #
    # The area's argv is the oracle schema's, not the moduledoc's: `create` takes
    # NAME TYPE URL **positionally** (`--name/--type/--url` are `invalid option`
    # there, and clap refuses the same three), while `update` keeps its three as
    # options. Every write route carries the `request_body` pin the capture
    # produced — including `--data`'s nesting under `"data"` and the token at
    # `authorization.parameters.accessToken` — so a body regression fails the
    # route's verdict even when both sides regress together. `--type` is a query
    # pair on the wire, not a client-side filter. The `--json` success paths are
    # the oracle's human line there and this build's value/message envelope (D33);
    # the module's own 404s and local errors (`update`'s empty-body guard, the
    # `--data` messages, an unreadable secret file) are stderr-only there and an
    # envelope here (D4); a missing positional is D5. The `delete` cases are the
    # wave's fourth prompt: `prompt-text`/`prompt-json` strip the oracle's question
    # and assert D31 — the question is on the oracle's stdout, this build's on
    # stderr — while `n`/EOF/`--force` carry D30/D32 as Wave 2 pinned them. The
    # last case is the one unported spelling this task found: the oracle's
    # booleans also take `--flag=false`/`--no-flag` and this build's clap flags do
    # not (recorded in the task report, not repaired here).

    expect_oracle_requests='length == 1 and any_path("/ado-harness/Alpha/_apis/serviceendpoint/endpoints") and qpair("api-version=7.1")'
    expect_rust_requests='length == 1 and any_path("/ado-harness/Alpha/_apis/serviceendpoint/endpoints") and qpair("api-version=7.1")'
    mock_case connections-list "connections list" \
        connections list Alpha --json

    expect_oracle_requests='length == 1 and any_path("/ado-harness/Alpha/_apis/serviceendpoint/endpoints") and qpair("api-version=7.1") and qpair("type=github")'
    expect_rust_requests='length == 1 and any_path("/ado-harness/Alpha/_apis/serviceendpoint/endpoints") and qpair("api-version=7.1") and qpair("type=github")'
    mock_case connections-list-type "connections list --type" \
        connections list Alpha --type github --json

    stdout_mode=text
    mock_case connections-list-empty-human "connections list (empty, human)" \
        connections list Empty

    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with inspect/2 (C12: the list-error pair)'
    expect_oracle_requests='length == 1 and any_path("/ado-harness/Missing/_apis/serviceendpoint/endpoints") and qpair("api-version=7.1")'
    expect_rust_requests='length == 1 and any_path("/ado-harness/Missing/_apis/serviceendpoint/endpoints") and qpair("api-version=7.1")'
    mock_case connections-list-404 "connections list (404)" \
        connections list Missing --json

    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with inspect/2 (C12: the list-error pair)'
    expect_oracle_requests='length == 1 and any_path("/ado-harness/Broken/_apis/serviceendpoint/endpoints") and qpair("api-version=7.1")'
    expect_rust_requests='length == 1 and any_path("/ado-harness/Broken/_apis/serviceendpoint/endpoints") and qpair("api-version=7.1")'
    mock_case connections-list-500 "connections list (500)" \
        connections list Broken --json

    envelope_rule='D4: the oracle prints its human error line on stdout; this build writes the labelled line to stderr alone'
    expect_oracle_requests='length == 1 and any_path("/ado-harness/Missing/_apis/serviceendpoint/endpoints")'
    expect_rust_requests='length == 1 and any_path("/ado-harness/Missing/_apis/serviceendpoint/endpoints")'
    stdout_mode=text
    mock_case connections-list-404-human "connections list (404, human)" \
        connections list Missing

    envelope_rule='D5: the oracle prints the command help on stdout before its usage error; this build writes clap’s message to stderr alone'
    stdout_mode=text
    mock_case connections-list-no-project "connections list (no project)" \
        connections list --json

    expect_oracle_requests='length == 1 and any_path("/ado-harness/Alpha/_apis/serviceendpoint/endpoints/c1") and qpair("api-version=7.1")'
    expect_rust_requests='length == 1 and any_path("/ado-harness/Alpha/_apis/serviceendpoint/endpoints/c1") and qpair("api-version=7.1")'
    mock_case connections-show "connections show" \
        connections show Alpha c1 --json

    stdout_mode=text
    mock_case connections-show-human "connections show (human)" \
        connections show Alpha c1

    envelope_rule='D4: the frozen CLI writes the 404 to stderr with no envelope under --json where this build emits the error envelope'
    expect_oracle_requests='length == 1 and any_path("/ado-harness/Alpha/_apis/serviceendpoint/endpoints/missing")'
    expect_rust_requests='length == 1 and any_path("/ado-harness/Alpha/_apis/serviceendpoint/endpoints/missing")'
    mock_case connections-show-404 "connections show (404)" \
        connections show Alpha missing --json

    envelope_rule='D5: the oracle prints the command help on stdout before its usage error; this build writes clap’s message to stderr alone'
    stdout_mode=text
    mock_case connections-show-no-id "connections show (no connection_id)" \
        connections show Alpha --json

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the value envelope'
    expect_oracle_requests='length == 1 and (.[0].method == "POST") and any_path("/ado-harness/Alpha/_apis/serviceendpoint/endpoints") and qpair("api-version=7.1")'
    expect_rust_requests='length == 1 and (.[0].method == "POST") and any_path("/ado-harness/Alpha/_apis/serviceendpoint/endpoints") and qpair("api-version=7.1")'
    mock_case connections-create "connections create" \
        connections create Alpha GitHub github https://github.com --json

    expect_oracle_requests='length == 1 and (.[0].method == "POST") and any_path("/ado-harness/Alpha/_apis/serviceendpoint/endpoints")'
    expect_rust_requests='length == 1 and (.[0].method == "POST") and any_path("/ado-harness/Alpha/_apis/serviceendpoint/endpoints")'
    stdout_mode=text
    mock_case connections-create-human "connections create (human)" \
        connections create Alpha GitHub github https://github.com

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the value envelope'
    expect_oracle_requests='length == 1 and (.[0].method == "POST") and any_path("/ado-harness/Full/_apis/serviceendpoint/endpoints")'
    expect_rust_requests='length == 1 and (.[0].method == "POST") and any_path("/ado-harness/Full/_apis/serviceendpoint/endpoints")'
    mock_case connections-create-full "connections create (every option)" \
        connections create Full GitHub github https://github.com \
        --description "A GitHub PAT" --scheme Token --access-token ghp_xxx \
        --data '{"subscriptionId":"s1"}' --ready --json

    case_stdin=$'ghp_from_stdin\n'
    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the value envelope'
    expect_oracle_requests='length == 1 and (.[0].method == "POST") and any_path("/ado-harness/Stdin/_apis/serviceendpoint/endpoints")'
    expect_rust_requests='length == 1 and (.[0].method == "POST") and any_path("/ado-harness/Stdin/_apis/serviceendpoint/endpoints")'
    mock_case connections-create-token-stdin "connections create --access-token -" \
        connections create Stdin GitHub github https://github.com --access-token - --json

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the value envelope'
    expect_oracle_requests='length == 1 and (.[0].method == "POST") and any_path("/ado-harness/TokenFile/_apis/serviceendpoint/endpoints")'
    expect_rust_requests='length == 1 and (.[0].method == "POST") and any_path("/ado-harness/TokenFile/_apis/serviceendpoint/endpoints")'
    mock_case connections-create-token-file "connections create --access-token @file" \
        connections create TokenFile GitHub github https://github.com \
        --access-token "@$root/crates/ado-testkit/fixtures/connection_token.txt" --json

    envelope_rule='D4: the frozen CLI writes the local error to stderr with no envelope under --json where this build emits the error envelope'
    mock_case connections-create-token-absent "connections create --access-token @absent" \
        connections create Alpha GitHub github https://github.com \
        --access-token "@$root/crates/ado-testkit/fixtures/absent.txt" --json

    envelope_rule='D4: the frozen CLI writes the local error to stderr with no envelope under --json where this build emits the error envelope'
    mock_case connections-create-data-invalid "connections create --data not-json" \
        connections create Alpha GitHub github https://github.com --data notjson --json

    envelope_rule='D4: the frozen CLI writes the 404 to stderr with no envelope under --json where this build emits the error envelope'
    expect_oracle_requests='length == 1 and (.[0].method == "POST") and any_path("/ado-harness/Missing/_apis/serviceendpoint/endpoints")'
    expect_rust_requests='length == 1 and (.[0].method == "POST") and any_path("/ado-harness/Missing/_apis/serviceendpoint/endpoints")'
    mock_case connections-create-404 "connections create (404)" \
        connections create Missing GitHub github https://github.com --json

    envelope_rule='D5: the oracle prints the command help on stdout before its usage error; this build writes clap’s message to stderr alone'
    stdout_mode=text
    mock_case connections-create-no-url "connections create (no url)" \
        connections create Alpha GitHub github --json

    status_rule='the unported boolean spelling: the oracle parses --ready=false, this build’s clap refuses it'
    expect_statuses='0 1'
    rest_rule='the unported boolean spelling: the oracle proceeds with isReady=false, this build refuses the flag and sends nothing'
    expect_oracle_requests='length == 1 and (.[0].method == "POST") and any_path("/ado-harness/Alpha/_apis/serviceendpoint/endpoints")'
    expect_rust_requests='length == 0'
    envelope_rule='D5: the oracle prints its human success line, this build writes clap’s usage error to stderr alone'
    stdout_mode=text
    mock_case connections-create-ready-eq-false "connections create --ready=false (unported boolean spelling)" \
        connections create Alpha GitHub github https://github.com --ready=false

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the value envelope'
    expect_oracle_requests='length == 1 and (.[0].method == "PUT") and any_path("/ado-harness/Alpha/_apis/serviceendpoint/endpoints/c1")'
    expect_rust_requests='length == 1 and (.[0].method == "PUT") and any_path("/ado-harness/Alpha/_apis/serviceendpoint/endpoints/c1")'
    mock_case connections-update "connections update --name" \
        connections update Alpha c1 --name Renamed --json

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the value envelope'
    expect_oracle_requests='length == 1 and (.[0].method == "PUT") and any_path("/ado-harness/Full/_apis/serviceendpoint/endpoints/c1")'
    expect_rust_requests='length == 1 and (.[0].method == "PUT") and any_path("/ado-harness/Full/_apis/serviceendpoint/endpoints/c1")'
    mock_case connections-update-full "connections update (every option)" \
        connections update Full c1 --name Renamed --description Renamed \
        --url https://github.com/new --access-token ghp_new \
        --data '{"subscriptionId":"s2"}' --json

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the value envelope'
    expect_oracle_requests='length == 1 and (.[0].method == "PUT") and any_path("/ado-harness/Token/_apis/serviceendpoint/endpoints/c1")'
    expect_rust_requests='length == 1 and (.[0].method == "PUT") and any_path("/ado-harness/Token/_apis/serviceendpoint/endpoints/c1")'
    mock_case connections-update-token "connections update --access-token" \
        connections update Token c1 --access-token ghp_new --json

    envelope_rule='D4: the frozen CLI writes the guard to stderr with no envelope under --json where this build emits the error envelope'
    mock_case connections-update-guard "connections update (no options)" \
        connections update Alpha c1 --json

    envelope_rule='D4: the frozen CLI writes the 404 to stderr with no envelope under --json where this build emits the error envelope'
    expect_oracle_requests='length == 1 and (.[0].method == "PUT") and any_path("/ado-harness/Alpha/_apis/serviceendpoint/endpoints/missing")'
    expect_rust_requests='length == 1 and (.[0].method == "PUT") and any_path("/ado-harness/Alpha/_apis/serviceendpoint/endpoints/missing")'
    mock_case connections-update-404 "connections update (404)" \
        connections update Alpha missing --name X --json

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the message envelope'
    expect_oracle_requests='length == 1 and (.[0].method == "DELETE") and any_path("/ado-harness/Alpha/_apis/serviceendpoint/endpoints/c1")'
    expect_rust_requests='length == 1 and (.[0].method == "DELETE") and any_path("/ado-harness/Alpha/_apis/serviceendpoint/endpoints/c1")'
    mock_case connections-delete-force "connections delete --force" \
        connections delete Alpha c1 --force --json

    expect_oracle_requests='length == 1 and (.[0].method == "DELETE") and any_path("/ado-harness/Alpha/_apis/serviceendpoint/endpoints/c1")'
    expect_rust_requests='length == 1 and (.[0].method == "DELETE") and any_path("/ado-harness/Alpha/_apis/serviceendpoint/endpoints/c1")'
    stdout_mode=text
    mock_case connections-delete-force-human "connections delete --force (human)" \
        connections delete Alpha c1 --force

    case_stdin=$'y\n'
    expect_oracle_requests='length == 1 and (.[0].method == "DELETE") and any_path("/ado-harness/Alpha/_apis/serviceendpoint/endpoints/c1")'
    expect_rust_requests='length == 1 and (.[0].method == "DELETE") and any_path("/ado-harness/Alpha/_apis/serviceendpoint/endpoints/c1")'
    stdout_mode=prompt-text
    mock_case connections-delete-confirmed "connections delete (confirmed)" \
        connections delete Alpha c1

    case_stdin=$'y\n'
    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the message envelope'
    expect_oracle_requests='length == 1 and (.[0].method == "DELETE") and any_path("/ado-harness/Alpha/_apis/serviceendpoint/endpoints/c1")'
    expect_rust_requests='length == 1 and (.[0].method == "DELETE") and any_path("/ado-harness/Alpha/_apis/serviceendpoint/endpoints/c1")'
    stdout_mode=prompt-json
    mock_case connections-delete-confirmed-json "connections delete (confirmed, --json)" \
        connections delete Alpha c1 --json

    case_stdin=$'n\n'
    stdout_mode=prompt-text
    mock_case connections-delete-refused "connections delete (refused)" \
        connections delete Alpha c1

    case_stdin=$'n\n'
    stdout_mode=prompt-text
    mock_case connections-delete-refused-json "connections delete (refused, --json)" \
        connections delete Alpha c1 --json

    status_rule='D30: the frozen CLI exits 0 on an unanswered prompt; this build refuses with exit 1'
    expect_statuses='0 1'
    stdout_mode=prompt-text
    mock_case connections-delete-eof "connections delete (EOF)" \
        connections delete Alpha c1

    envelope_rule='D4: the frozen CLI writes the 404 to stderr with no envelope under --json where this build emits the error envelope'
    expect_oracle_requests='length == 1 and (.[0].method == "DELETE") and any_path("/ado-harness/Alpha/_apis/serviceendpoint/endpoints/missing")'
    expect_rust_requests='length == 1 and (.[0].method == "DELETE") and any_path("/ado-harness/Alpha/_apis/serviceendpoint/endpoints/missing")'
    mock_case connections-delete-404 "connections delete (404)" \
        connections delete Alpha missing --force --json

    # ── Wave 3: marketplace extensions (Task 4) ──
    #
    # The six commands are the frozen schema's argv (Annex A agrees with the
    # capture): `--search`, `--publisher` and `--name` are options, `extension_id`
    # the one positional. `--search` filters client-side on `extensionName` alone,
    # so the list cases assert "one request, no filter pair" with a direction
    # filter that pins the query to its single pair, and the three search cases
    # carry the envelope claim beside it (a server-side filter would answer the
    # unfiltered array). `install`/`enable`/`disable`/`uninstall` print their human
    # success line under `--json` there and this build's message envelope here
    # (D33); each write route's `request_body` pin holds the captured body, and
    # enable/disable take different paths because one scenario path can carry one
    # body pin and the `installState.flags` value is the difference being pinned.
    # `show`/`uninstall`'s 404s keep the module's own wording (D4's halt_error
    # class), `enable`/`disable`'s are the classified envelope (D24's body
    # rendering); a missing required option is D34's silent exit 0, a missing
    # positional D5. The two D22 sites (`show`'s `URI.encode/1`, the writes' raw
    # dotted id) carry no case — recorded in the D22 row and integration-pinned,
    # Task 2/3's precedent for a spelling no other case exercises.

    expect_oracle_requests='length == 1 and any_path("/ado-harness/_apis/extensionmanagement/installedextensions") and qpair("api-version=7.1") and (.[0].query | split("&") | length == 1)'
    expect_rust_requests='length == 1 and any_path("/ado-harness/_apis/extensionmanagement/installedextensions") and qpair("api-version=7.1") and (.[0].query | split("&") | length == 1)'
    mock_case extensions-list "extensions list" \
        extensions list --json

    case_org=empty-org
    expect_oracle_requests='length == 1 and any_path("/empty-org/_apis/extensionmanagement/installedextensions") and qpair("api-version=7.1") and (.[0].query | split("&") | length == 1)'
    expect_rust_requests='length == 1 and any_path("/empty-org/_apis/extensionmanagement/installedextensions") and qpair("api-version=7.1") and (.[0].query | split("&") | length == 1)'
    stdout_mode=text
    mock_case extensions-list-empty-human "extensions list (empty org, human)" \
        extensions list

    case_org=ado-harness
    expect_oracle_requests='length == 1 and any_path("/ado-harness/_apis/extensionmanagement/installedextensions") and qpair("api-version=7.1") and (.[0].query | split("&") | length == 1)'
    expect_rust_requests='length == 1 and any_path("/ado-harness/_apis/extensionmanagement/installedextensions") and qpair("api-version=7.1") and (.[0].query | split("&") | length == 1)'
    mock_case extensions-list-search "extensions list --search (case-insensitive)" \
        extensions list --search Build --json

    expect_oracle_requests='length == 1 and any_path("/ado-harness/_apis/extensionmanagement/installedextensions") and qpair("api-version=7.1") and (.[0].query | split("&") | length == 1)'
    expect_rust_requests='length == 1 and any_path("/ado-harness/_apis/extensionmanagement/installedextensions") and qpair("api-version=7.1") and (.[0].query | split("&") | length == 1)'
    mock_case extensions-list-search-empty "extensions list --search '' (keeps everything)" \
        extensions list --search '' --json

    expect_oracle_requests='length == 1 and any_path("/ado-harness/_apis/extensionmanagement/installedextensions") and qpair("api-version=7.1") and (.[0].query | split("&") | length == 1)'
    expect_rust_requests='length == 1 and any_path("/ado-harness/_apis/extensionmanagement/installedextensions") and qpair("api-version=7.1") and (.[0].query | split("&") | length == 1)'
    mock_case extensions-list-search-publisher "extensions list --search (the publisher, not the name)" \
        extensions list --search mspremier --json

    case_org=missing
    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with inspect/2 (C12: the list-error row)'
    expect_oracle_requests='length == 1 and any_path("/missing/_apis/extensionmanagement/installedextensions") and qpair("api-version=7.1")'
    expect_rust_requests='length == 1 and any_path("/missing/_apis/extensionmanagement/installedextensions") and qpair("api-version=7.1")'
    mock_case extensions-list-404 "extensions list (404)" \
        extensions list --json

    case_org=broken
    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with inspect/2 (C12: the list-error row)'
    expect_oracle_requests='length == 1 and any_path("/broken/_apis/extensionmanagement/installedextensions") and qpair("api-version=7.1")'
    expect_rust_requests='length == 1 and any_path("/broken/_apis/extensionmanagement/installedextensions") and qpair("api-version=7.1")'
    mock_case extensions-list-500 "extensions list (500)" \
        extensions list --json

    case_org=missing
    envelope_rule='D4: the oracle prints its human error line on stdout; this build writes the labelled line to stderr alone'
    expect_oracle_requests='length == 1 and any_path("/missing/_apis/extensionmanagement/installedextensions") and qpair("api-version=7.1")'
    expect_rust_requests='length == 1 and any_path("/missing/_apis/extensionmanagement/installedextensions") and qpair("api-version=7.1")'
    stdout_mode=text
    mock_case extensions-list-404-human "extensions list (404, human)" \
        extensions list

    case_org=broken
    envelope_rule='D4: the oracle prints its human error line on stdout; this build writes the labelled line to stderr alone'
    expect_oracle_requests='length == 1 and any_path("/broken/_apis/extensionmanagement/installedextensions") and qpair("api-version=7.1")'
    expect_rust_requests='length == 1 and any_path("/broken/_apis/extensionmanagement/installedextensions") and qpair("api-version=7.1")'
    stdout_mode=text
    mock_case extensions-list-500-human "extensions list (500, human)" \
        extensions list

    case_org=ado-harness
    expect_oracle_requests='length == 1 and any_path("/ado-harness/_apis/extensionmanagement/installedextensions/mspremier.BuildQualityChecks") and qpair("api-version=7.1")'
    expect_rust_requests='length == 1 and any_path("/ado-harness/_apis/extensionmanagement/installedextensions/mspremier.BuildQualityChecks") and qpair("api-version=7.1")'
    mock_case extensions-show "extensions show" \
        extensions show mspremier.BuildQualityChecks --json

    stdout_mode=text
    mock_case extensions-show-human "extensions show (human)" \
        extensions show mspremier.BuildQualityChecks

    envelope_rule='D4: the frozen CLI writes the module wording to stderr with no envelope under --json where this build emits the error envelope'
    expect_oracle_requests='length == 1 and any_path("/ado-harness/_apis/extensionmanagement/installedextensions/missing.thing")'
    expect_rust_requests='length == 1 and any_path("/ado-harness/_apis/extensionmanagement/installedextensions/missing.thing")'
    mock_case extensions-show-404 "extensions show (404)" \
        extensions show missing.thing --json

    envelope_rule='D5: the oracle prints the command help on stdout before its usage error; this build writes clap’s message to stderr alone'
    stdout_mode=text
    mock_case extensions-show-no-id "extensions show (no extension_id)" \
        extensions show --json

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the message envelope'
    expect_oracle_requests='length == 1 and (.[0].method == "POST") and any_path("/ado-harness/_apis/extensionmanagement/installedextensions") and qpair("api-version=7.1")'
    expect_rust_requests='length == 1 and (.[0].method == "POST") and any_path("/ado-harness/_apis/extensionmanagement/installedextensions") and qpair("api-version=7.1")'
    mock_case extensions-install "extensions install" \
        extensions install --publisher mspremier --name BuildQualityChecks --json

    expect_oracle_requests='length == 1 and (.[0].method == "POST") and any_path("/ado-harness/_apis/extensionmanagement/installedextensions") and qpair("api-version=7.1")'
    expect_rust_requests='length == 1 and (.[0].method == "POST") and any_path("/ado-harness/_apis/extensionmanagement/installedextensions") and qpair("api-version=7.1")'
    stdout_mode=text
    mock_case extensions-install-human "extensions install (human)" \
        extensions install --publisher mspremier --name BuildQualityChecks

    case_org=fail-install
    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with inspect/2'
    expect_oracle_requests='length == 1 and (.[0].method == "POST") and any_path("/fail-install/_apis/extensionmanagement/installedextensions") and qpair("api-version=7.1")'
    expect_rust_requests='length == 1 and (.[0].method == "POST") and any_path("/fail-install/_apis/extensionmanagement/installedextensions") and qpair("api-version=7.1")'
    mock_case extensions-install-400 "extensions install (400)" \
        extensions install --publisher mspremier --name BuildQualityChecks --json

    case_org=ado-harness
    status_rule='D34: a missing required option is a silent exit 0 in the oracle (the module’s Map.fetch! crash); this build is a loud usage error'
    expect_statuses='0 1'
    stdout_mode=text
    mock_case extensions-install-no-publisher "extensions install (no --publisher)" \
        extensions install --name BuildQualityChecks --json

    status_rule='D34: a missing required option is a silent exit 0 in the oracle (the module’s Map.fetch! crash); this build is a loud usage error'
    expect_statuses='0 1'
    stdout_mode=text
    mock_case extensions-install-no-name "extensions install (no --name)" \
        extensions install --publisher mspremier --json

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the message envelope'
    expect_oracle_requests='length == 1 and (.[0].method == "DELETE") and any_path("/ado-harness/_apis/extensionmanagement/installedextensions/mspremier.BuildQualityChecks") and qpair("api-version=7.1")'
    expect_rust_requests='length == 1 and (.[0].method == "DELETE") and any_path("/ado-harness/_apis/extensionmanagement/installedextensions/mspremier.BuildQualityChecks") and qpair("api-version=7.1")'
    mock_case extensions-uninstall "extensions uninstall" \
        extensions uninstall --publisher mspremier --name BuildQualityChecks --json

    expect_oracle_requests='length == 1 and (.[0].method == "DELETE") and any_path("/ado-harness/_apis/extensionmanagement/installedextensions/mspremier.BuildQualityChecks") and qpair("api-version=7.1")'
    expect_rust_requests='length == 1 and (.[0].method == "DELETE") and any_path("/ado-harness/_apis/extensionmanagement/installedextensions/mspremier.BuildQualityChecks") and qpair("api-version=7.1")'
    stdout_mode=text
    mock_case extensions-uninstall-human "extensions uninstall (human)" \
        extensions uninstall --publisher mspremier --name BuildQualityChecks

    envelope_rule='D4: the frozen CLI writes the module wording to stderr with no envelope under --json where this build emits the error envelope'
    expect_oracle_requests='length == 1 and (.[0].method == "DELETE") and any_path("/ado-harness/_apis/extensionmanagement/installedextensions/missing.thing")'
    expect_rust_requests='length == 1 and (.[0].method == "DELETE") and any_path("/ado-harness/_apis/extensionmanagement/installedextensions/missing.thing")'
    mock_case extensions-uninstall-404 "extensions uninstall (404)" \
        extensions uninstall --publisher missing --name thing --json

    status_rule='D34: a missing required option is a silent exit 0 in the oracle (the module’s Map.fetch! crash); this build is a loud usage error'
    expect_statuses='0 1'
    stdout_mode=text
    mock_case extensions-uninstall-no-publisher "extensions uninstall (no --publisher)" \
        extensions uninstall --name BuildQualityChecks --json

    status_rule='D34: a missing required option is a silent exit 0 in the oracle (the module’s Map.fetch! crash); this build is a loud usage error'
    expect_statuses='0 1'
    stdout_mode=text
    mock_case extensions-uninstall-no-name "extensions uninstall (no --name)" \
        extensions uninstall --publisher mspremier --json

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the message envelope'
    expect_oracle_requests='length == 1 and (.[0].method == "PATCH") and any_path("/ado-harness/_apis/extensionmanagement/installedextensions/mspremier.BuildQualityChecks") and qpair("api-version=7.1")'
    expect_rust_requests='length == 1 and (.[0].method == "PATCH") and any_path("/ado-harness/_apis/extensionmanagement/installedextensions/mspremier.BuildQualityChecks") and qpair("api-version=7.1")'
    mock_case extensions-enable "extensions enable" \
        extensions enable --publisher mspremier --name BuildQualityChecks --json

    expect_oracle_requests='length == 1 and (.[0].method == "PATCH") and any_path("/ado-harness/_apis/extensionmanagement/installedextensions/mspremier.BuildQualityChecks") and qpair("api-version=7.1")'
    expect_rust_requests='length == 1 and (.[0].method == "PATCH") and any_path("/ado-harness/_apis/extensionmanagement/installedextensions/mspremier.BuildQualityChecks") and qpair("api-version=7.1")'
    stdout_mode=text
    mock_case extensions-enable-human "extensions enable (human)" \
        extensions enable --publisher mspremier --name BuildQualityChecks

    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with inspect/2'
    expect_oracle_requests='length == 1 and (.[0].method == "PATCH") and any_path("/ado-harness/_apis/extensionmanagement/installedextensions/missing.thing")'
    expect_rust_requests='length == 1 and (.[0].method == "PATCH") and any_path("/ado-harness/_apis/extensionmanagement/installedextensions/missing.thing")'
    mock_case extensions-enable-404 "extensions enable (404)" \
        extensions enable --publisher missing --name thing --json

    status_rule='D34: a missing required option is a silent exit 0 in the oracle (the module’s Map.fetch! crash); this build is a loud usage error'
    expect_statuses='0 1'
    stdout_mode=text
    mock_case extensions-enable-no-publisher "extensions enable (no --publisher)" \
        extensions enable --name BuildQualityChecks --json

    status_rule='D34: a missing required option is a silent exit 0 in the oracle (the module’s Map.fetch! crash); this build is a loud usage error'
    expect_statuses='0 1'
    stdout_mode=text
    mock_case extensions-enable-no-name "extensions enable (no --name)" \
        extensions enable --publisher mspremier --json

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the message envelope'
    expect_oracle_requests='length == 1 and (.[0].method == "PATCH") and any_path("/ado-harness/_apis/extensionmanagement/installedextensions/octopus.octopus-deploy") and qpair("api-version=7.1")'
    expect_rust_requests='length == 1 and (.[0].method == "PATCH") and any_path("/ado-harness/_apis/extensionmanagement/installedextensions/octopus.octopus-deploy") and qpair("api-version=7.1")'
    mock_case extensions-disable "extensions disable (the disabled flags body)" \
        extensions disable --publisher octopus --name octopus-deploy --json

    expect_oracle_requests='length == 1 and (.[0].method == "PATCH") and any_path("/ado-harness/_apis/extensionmanagement/installedextensions/octopus.octopus-deploy") and qpair("api-version=7.1")'
    expect_rust_requests='length == 1 and (.[0].method == "PATCH") and any_path("/ado-harness/_apis/extensionmanagement/installedextensions/octopus.octopus-deploy") and qpair("api-version=7.1")'
    stdout_mode=text
    mock_case extensions-disable-human "extensions disable (human)" \
        extensions disable --publisher octopus --name octopus-deploy

    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with inspect/2'
    expect_oracle_requests='length == 1 and (.[0].method == "PATCH") and any_path("/ado-harness/_apis/extensionmanagement/installedextensions/missing.thing")'
    expect_rust_requests='length == 1 and (.[0].method == "PATCH") and any_path("/ado-harness/_apis/extensionmanagement/installedextensions/missing.thing")'
    mock_case extensions-disable-404 "extensions disable (404)" \
        extensions disable --publisher missing --name thing --json

    status_rule='D34: a missing required option is a silent exit 0 in the oracle (the module’s Map.fetch! crash); this build is a loud usage error'
    expect_statuses='0 1'
    stdout_mode=text
    mock_case extensions-disable-no-publisher "extensions disable (no --publisher)" \
        extensions disable --name BuildQualityChecks --json

    status_rule='D34: a missing required option is a silent exit 0 in the oracle (the module’s Map.fetch! crash); this build is a loud usage error'
    expect_statuses='0 1'
    stdout_mode=text
    mock_case extensions-disable-no-name "extensions disable (no --name)" \
        extensions disable --publisher mspremier --json

    # ── Wave 3: imports and organization banners (Task 5) ──
    #
    # The request surface is the modules': `imports list`'s `$top` pair (present
    # for `0` too, because `0` is truthy in Elixir), `imports create`'s two-level
    # body with the credential fields only when their options are given, and the
    # org-scoped banners settings entry. Every case states both sides' spelling in
    # the direction filters (C6). The two lists carry C12's 404/500 pair, in both
    # modes. `banners show`'s 404 is not an error — the oracle answers its human
    # sentence, exit 0, even under `--json`, where this build emits the empty-value
    # envelope — and `banners set --message`'s `@<file>`/`-` forms are Ruling 4(b)'s
    # repair, so their case pins D44: the oracle sends the literal string, this
    # build reads the file/stdin.

    expect_oracle_requests='length == 1 and (.[0].method == "GET") and any_path("/ado-harness/Alpha/_apis/git/importRequests") and qpair("api-version=7.1") and (.[0].query | split("&") | length == 1)'
    expect_rust_requests='length == 1 and (.[0].method == "GET") and any_path("/ado-harness/Alpha/_apis/git/importRequests") and qpair("api-version=7.1") and (.[0].query | split("&") | length == 1)'
    mock_case imports-list "imports list" \
        imports list Alpha --json

    expect_oracle_requests='length == 1 and any_path("/ado-harness/Empty/_apis/git/importRequests") and qpair("api-version=7.1")'
    expect_rust_requests='length == 1 and any_path("/ado-harness/Empty/_apis/git/importRequests") and qpair("api-version=7.1")'
    stdout_mode=text
    mock_case imports-list-empty-human "imports list (empty project, human)" \
        imports list Empty

    expect_oracle_requests='length == 1 and any_path("/ado-harness/Alpha/_apis/git/importRequests") and qpair("%24top=1") and qpair("api-version=7.1")'
    expect_rust_requests='length == 1 and any_path("/ado-harness/Alpha/_apis/git/importRequests") and qpair("%24top=1") and qpair("api-version=7.1")'
    mock_case imports-list-top "imports list --top 1" \
        imports list Alpha --top 1 --json

    expect_oracle_requests='length == 1 and any_path("/ado-harness/Alpha/_apis/git/importRequests") and qpair("%24top=0") and qpair("api-version=7.1")'
    expect_rust_requests='length == 1 and any_path("/ado-harness/Alpha/_apis/git/importRequests") and qpair("%24top=0") and qpair("api-version=7.1")'
    mock_case imports-list-top-zero "imports list --top 0 (zero is a present option)" \
        imports list Alpha --top 0 --json

    case_org=missing
    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with inspect/2 (C12: the list-error row)'
    expect_oracle_requests='length == 1 and any_path("/missing/Alpha/_apis/git/importRequests") and qpair("api-version=7.1")'
    expect_rust_requests='length == 1 and any_path("/missing/Alpha/_apis/git/importRequests") and qpair("api-version=7.1")'
    mock_case imports-list-404 "imports list (404)" \
        imports list Alpha --json

    case_org=broken
    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with inspect/2 (C12: the list-error row)'
    expect_oracle_requests='length == 1 and any_path("/broken/Alpha/_apis/git/importRequests") and qpair("api-version=7.1")'
    expect_rust_requests='length == 1 and any_path("/broken/Alpha/_apis/git/importRequests") and qpair("api-version=7.1")'
    mock_case imports-list-500 "imports list (500)" \
        imports list Alpha --json

    case_org=missing
    envelope_rule='D4: the oracle prints its human error line on stdout; this build writes the labelled line to stderr alone'
    expect_oracle_requests='length == 1 and any_path("/missing/Alpha/_apis/git/importRequests") and qpair("api-version=7.1")'
    expect_rust_requests='length == 1 and any_path("/missing/Alpha/_apis/git/importRequests") and qpair("api-version=7.1")'
    stdout_mode=text
    mock_case imports-list-404-human "imports list (404, human)" \
        imports list Alpha

    case_org=broken
    envelope_rule='D4: the oracle prints its human error line on stdout; this build writes the labelled line to stderr alone'
    stdout_mode=text
    mock_case imports-list-500-human "imports list (500, human)" \
        imports list Alpha

    case_org=ado-harness
    envelope_rule='D5: the oracle prints the command help on stdout before its usage error; this build writes clap’s message to stderr alone'
    stdout_mode=text
    mock_case imports-list-no-project "imports list (no project)" \
        imports list --json

    expect_oracle_requests='length == 1 and any_path("/ado-harness/Alpha/_apis/git/importRequests/imp-1") and qpair("api-version=7.1")'
    expect_rust_requests='length == 1 and any_path("/ado-harness/Alpha/_apis/git/importRequests/imp-1") and qpair("api-version=7.1")'
    mock_case imports-show "imports show" \
        imports show Alpha imp-1 --json

    expect_oracle_requests='length == 1 and any_path("/ado-harness/Alpha/_apis/git/importRequests/imp-1") and qpair("api-version=7.1")'
    expect_rust_requests='length == 1 and any_path("/ado-harness/Alpha/_apis/git/importRequests/imp-1") and qpair("api-version=7.1")'
    stdout_mode=text
    mock_case imports-show-human "imports show (human)" \
        imports show Alpha imp-1

    expect_oracle_requests='length == 1 and any_path("/ado-harness/Alpha/_apis/git/importRequests/imp-2") and qpair("api-version=7.1")'
    expect_rust_requests='length == 1 and any_path("/ado-harness/Alpha/_apis/git/importRequests/imp-2") and qpair("api-version=7.1")'
    stdout_mode=text
    mock_case imports-show-falsy-detail "imports show (the falsy detail line, human)" \
        imports show Alpha imp-2

    envelope_rule='D4: the frozen CLI writes the module wording to stderr with no envelope under --json where this build emits the error envelope'
    expect_oracle_requests='length == 1 and any_path("/ado-harness/Alpha/_apis/git/importRequests/missing")'
    expect_rust_requests='length == 1 and any_path("/ado-harness/Alpha/_apis/git/importRequests/missing")'
    mock_case imports-show-404 "imports show (404)" \
        imports show Alpha missing --json

    envelope_rule='D33: the frozen create prints its human block under --json; this build emits the value envelope'
    expect_oracle_requests='length == 1 and (.[0].method == "POST") and any_path("/ado-harness/Alpha/_apis/git/repositories/NewRepo/importRequests") and qpair("api-version=7.1")'
    expect_rust_requests='length == 1 and (.[0].method == "POST") and any_path("/ado-harness/Alpha/_apis/git/repositories/NewRepo/importRequests") and qpair("api-version=7.1")'
    mock_case imports-create "imports create" \
        imports create Alpha NewRepo --url https://github.com/owner/repo.git --json

    expect_oracle_requests='length == 1 and (.[0].method == "POST") and any_path("/ado-harness/Alpha/_apis/git/repositories/NewRepo/importRequests") and qpair("api-version=7.1")'
    expect_rust_requests='length == 1 and (.[0].method == "POST") and any_path("/ado-harness/Alpha/_apis/git/repositories/NewRepo/importRequests") and qpair("api-version=7.1")'
    stdout_mode=text
    mock_case imports-create-human "imports create (human)" \
        imports create Alpha NewRepo --url https://github.com/owner/repo.git

    envelope_rule='D33: the frozen create prints its human block under --json; this build emits the value envelope'
    expect_oracle_requests='length == 1 and (.[0].method == "POST") and any_path("/ado-harness/Alpha/_apis/git/repositories/PrivateRepo/importRequests") and qpair("api-version=7.1")'
    expect_rust_requests='length == 1 and (.[0].method == "POST") and any_path("/ado-harness/Alpha/_apis/git/repositories/PrivateRepo/importRequests") and qpair("api-version=7.1")'
    mock_case imports-create-credentials "imports create (the credential fields)" \
        imports create Alpha PrivateRepo --url https://github.com/owner/repo.git --user octocat --password ghp_secret --json

    envelope_rule='D33: the frozen create prints its human block under --json; this build emits the value envelope'
    expect_oracle_requests='length == 1 and (.[0].method == "POST") and any_path("/ado-harness/Alpha/_apis/git/repositories/EmptyUrlRepo/importRequests") and qpair("api-version=7.1")'
    expect_rust_requests='length == 1 and (.[0].method == "POST") and any_path("/ado-harness/Alpha/_apis/git/repositories/EmptyUrlRepo/importRequests") and qpair("api-version=7.1")'
    mock_case imports-create-empty-url "imports create (a present empty --url)" \
        imports create Alpha EmptyUrlRepo --url '' --json

    status_rule='D34: a missing required option is a silent exit 0 in the oracle (the module’s Map.fetch! crash); this build is a loud usage error'
    expect_statuses='0 1'
    stdout_mode=text
    mock_case imports-create-no-url "imports create (no --url)" \
        imports create Alpha NewRepo --json

    case_org=missing
    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with inspect/2'
    expect_oracle_requests='length == 1 and (.[0].method == "POST") and any_path("/missing/Alpha/_apis/git/repositories/NewRepo/importRequests")'
    expect_rust_requests='length == 1 and (.[0].method == "POST") and any_path("/missing/Alpha/_apis/git/repositories/NewRepo/importRequests")'
    mock_case imports-create-404 "imports create (404)" \
        imports create Alpha NewRepo --url https://github.com/owner/repo.git --json

    case_org=broken
    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with inspect/2 (the message carries it too)'
    expect_oracle_requests='length == 1 and (.[0].method == "POST") and any_path("/broken/Alpha/_apis/git/repositories/NewRepo/importRequests")'
    expect_rust_requests='length == 1 and (.[0].method == "POST") and any_path("/broken/Alpha/_apis/git/repositories/NewRepo/importRequests")'
    mock_case imports-create-400 "imports create (400)" \
        imports create Alpha NewRepo --url https://github.com/owner/repo.git --json

    case_org=ado-harness
    expect_oracle_requests='length == 1 and any_path("/ado-harness/_apis/settings/entries/banners") and qpair("api-version=7.1")'
    expect_rust_requests='length == 1 and any_path("/ado-harness/_apis/settings/entries/banners") and qpair("api-version=7.1")'
    mock_case banners-show "banners show" \
        banners show --json

    expect_oracle_requests='length == 1 and any_path("/ado-harness/_apis/settings/entries/banners") and qpair("api-version=7.1")'
    expect_rust_requests='length == 1 and any_path("/ado-harness/_apis/settings/entries/banners") and qpair("api-version=7.1")'
    stdout_mode=text
    mock_case banners-show-human "banners show (human)" \
        banners show

    case_org=bare-org
    expect_oracle_requests='length == 1 and any_path("/bare-org/_apis/settings/entries/banners") and qpair("api-version=7.1")'
    expect_rust_requests='length == 1 and any_path("/bare-org/_apis/settings/entries/banners") and qpair("api-version=7.1")'
    mock_case banners-show-empty "banners show (an empty value)" \
        banners show --json

    case_org=bare-org
    expect_oracle_requests='length == 1 and any_path("/bare-org/_apis/settings/entries/banners") and qpair("api-version=7.1")'
    expect_rust_requests='length == 1 and any_path("/bare-org/_apis/settings/entries/banners") and qpair("api-version=7.1")'
    stdout_mode=text
    mock_case banners-show-empty-human "banners show (an empty value, human)" \
        banners show

    case_org=missing
    envelope_rule='D4: the oracle’s halt_success path prints its human sentence under --json; this build emits the empty-value envelope'
    expect_oracle_requests='length == 1 and any_path("/missing/_apis/settings/entries/banners") and qpair("api-version=7.1")'
    expect_rust_requests='length == 1 and any_path("/missing/_apis/settings/entries/banners") and qpair("api-version=7.1")'
    mock_case banners-show-404 "banners show (404 is not an error)" \
        banners show --json

    case_org=missing
    expect_oracle_requests='length == 1 and any_path("/missing/_apis/settings/entries/banners") and qpair("api-version=7.1")'
    expect_rust_requests='length == 1 and any_path("/missing/_apis/settings/entries/banners") and qpair("api-version=7.1")'
    stdout_mode=text
    mock_case banners-show-404-human "banners show (404, human)" \
        banners show

    case_org=broken
    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with inspect/2'
    expect_oracle_requests='length == 1 and any_path("/broken/_apis/settings/entries/banners") and qpair("api-version=7.1")'
    expect_rust_requests='length == 1 and any_path("/broken/_apis/settings/entries/banners") and qpair("api-version=7.1")'
    mock_case banners-show-500 "banners show (500)" \
        banners show --json

    case_org=ado-harness
    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the message envelope'
    expect_oracle_requests='length == 1 and (.[0].method == "PUT") and any_path("/ado-harness/_apis/settings/entries/banners") and qpair("api-version=7.1")'
    expect_rust_requests='length == 1 and (.[0].method == "PUT") and any_path("/ado-harness/_apis/settings/entries/banners") and qpair("api-version=7.1")'
    mock_case banners-set "banners set" \
        banners set --message 'Maintenance tonight' --json

    expect_oracle_requests='length == 1 and (.[0].method == "PUT") and any_path("/ado-harness/_apis/settings/entries/banners") and qpair("api-version=7.1")'
    expect_rust_requests='length == 1 and (.[0].method == "PUT") and any_path("/ado-harness/_apis/settings/entries/banners") and qpair("api-version=7.1")'
    stdout_mode=text
    mock_case banners-set-human "banners set (human)" \
        banners set --message 'Maintenance tonight'

    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the message envelope'
    expect_oracle_requests='length == 1 and (.[0].method == "PUT") and any_path("/ado-harness/_apis/settings/entries/banners") and qpair("api-version=7.1")'
    expect_rust_requests='length == 1 and (.[0].method == "PUT") and any_path("/ado-harness/_apis/settings/entries/banners") and qpair("api-version=7.1")'
    mock_case banners-set-type-level "banners set --type --level" \
        banners set --message 'Maintenance tonight' --type warning --level project --json

    # Ruling 4(b): the oracle sends the literal `@<path>` string; this build reads
    # the file (the `connections --access-token` convention), so the envelopes
    # differ by the message and the requests differ by the whole body — one D44 row.
    rest_rule='D44: the frozen `banners set` sends `@<path>` literally; this build reads the file (Ruling 4(b))'
    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the message envelope (naming the resolved message)'
    expect_oracle_requests='length == 1 and (.[0].method == "PUT") and any_body("banner-message.txt") and any_body("@")'
    expect_rust_requests='length == 1 and (.[0].method == "PUT") and any_body("Message from a file.") and (any_body("@") | not)'
    mock_case banners-set-at-file "banners set --message @<file>" \
        banners set --message "@$work/banner-message.txt" --json

    rest_rule='D44: the frozen `banners set` sends `-` literally; this build reads stdin (Ruling 4(b))'
    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the message envelope (naming the resolved message)'
    expect_oracle_requests='length == 1 and (.[0].method == "PUT") and any_body("\"message\":\"-\"")'
    expect_rust_requests='length == 1 and (.[0].method == "PUT") and any_body("From stdin.")'
    case_stdin=$'From stdin.'
    mock_case banners-set-dash "banners set --message -" \
        banners set --message - --json

    status_rule='D34: a missing required option is a silent exit 0 in the oracle (the module’s Map.fetch! crash); this build is a loud usage error'
    expect_statuses='0 1'
    stdout_mode=text
    mock_case banners-set-no-message "banners set (no --message)" \
        banners set --json

    case_org=missing
    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with inspect/2'
    expect_oracle_requests='length == 1 and (.[0].method == "PUT") and any_path("/missing/_apis/settings/entries/banners")'
    expect_rust_requests='length == 1 and (.[0].method == "PUT") and any_path("/missing/_apis/settings/entries/banners")'
    mock_case banners-set-404 "banners set (404)" \
        banners set --message 'Maintenance tonight' --json

    case_org=ado-harness
    envelope_rule='D33: the frozen write paths print their human success line under --json; this build emits the message envelope'
    expect_oracle_requests='length == 1 and (.[0].method == "DELETE") and any_path("/ado-harness/_apis/settings/entries/banners") and qpair("api-version=7.1")'
    expect_rust_requests='length == 1 and (.[0].method == "DELETE") and any_path("/ado-harness/_apis/settings/entries/banners") and qpair("api-version=7.1")'
    mock_case banners-delete "banners delete" \
        banners delete --json

    expect_oracle_requests='length == 1 and (.[0].method == "DELETE") and any_path("/ado-harness/_apis/settings/entries/banners") and qpair("api-version=7.1")'
    expect_rust_requests='length == 1 and (.[0].method == "DELETE") and any_path("/ado-harness/_apis/settings/entries/banners") and qpair("api-version=7.1")'
    stdout_mode=text
    mock_case banners-delete-human "banners delete (human)" \
        banners delete

    case_org=missing
    envelope_rule='D4: the frozen CLI writes the module wording to stderr with no envelope under --json where this build emits the error envelope'
    expect_oracle_requests='length == 1 and (.[0].method == "DELETE") and any_path("/missing/_apis/settings/entries/banners")'
    expect_rust_requests='length == 1 and (.[0].method == "DELETE") and any_path("/missing/_apis/settings/entries/banners")'
    mock_case banners-delete-404 "banners delete (404)" \
        banners delete --json

    case_org=broken
    # No rule: the frozen DELETE path keeps the upstream bytes (unlike its GET/PUT
    # paths, which re-render the decoded map with inspect/2), so this envelope is
    # byte-identical after jq -S and the case MATCHes.
    expect_oracle_requests='length == 1 and (.[0].method == "DELETE") and any_path("/broken/_apis/settings/entries/banners")'
    expect_rust_requests='length == 1 and (.[0].method == "DELETE") and any_path("/broken/_apis/settings/entries/banners")'
    mock_case banners-delete-500 "banners delete (500)" \
        banners delete --json

    # ── Wave 3: test results and the three repaired filters (Task 6) ──
    #
    # The request surface is the module's: `list`'s `$top` (present for `0` and a
    # negative — `0` is truthy in Elixir and its `OptionParser` takes a negative),
    # the two repaired filters (`buildIds`/`minLastUpdatedDate`; the oracle's
    # hyphen-declared declarations can never match, so it refuses them), the
    # `{project}/_apis/test/runs` collection and the run path under it. Every case
    # that reaches the wire states both sides' spelling in the direction filters
    # (C6), the list carries C12's 404/500 pair in both modes, and `publish`'s
    # third request carries the D25-family upload rule: the frozen `Client.post/3`
    # takes its content-type map as query **params** (glued after the path's own
    # `?api-version=7.1-preview.1&fileName=…`) and JSON-encodes the file into a
    # JSON string, where this build sends the two pairs and the bytes.
    tr_upload_norm='map(if (.path | endswith("/attachments")) then .query |= ([.[] | if contains("?Content-Type=") then split("?Content-Type=")[0] else . end] | map(select(startswith("Content-Type=") | not)) | map(select(. != "api-version=7.1")) | sort) else . end)'
    tr_upload_rule='D25 and the module’s own intent: the frozen upload glues `Content-Type` and a second `api-version` onto a path whose query already carries the preview version and the basename, and sends the file as a JSON string; this build sends the two pairs and the raw bytes'

    expect_oracle_requests='length == 1 and any_path("/ado-harness/Alpha/_apis/test/runs") and qpair("api-version=7.1") and (.[0].query | split("&") | length == 1)'
    expect_rust_requests='length == 1 and any_path("/ado-harness/Alpha/_apis/test/runs") and qpair("api-version=7.1") and (.[0].query | split("&") | length == 1)'
    mock_case test-results-list "test-results list" \
        test-results list Alpha --json

    envelope_rule='§8: the frozen table is its own rendering (8/40/12 pads, a 90-character rule, a leading blank line); this build renders the wave’s content-width table'
    expect_oracle_requests='length == 1 and any_path("/ado-harness/Alpha/_apis/test/runs") and qpair("api-version=7.1")'
    expect_rust_requests='length == 1 and any_path("/ado-harness/Alpha/_apis/test/runs") and qpair("api-version=7.1")'
    stdout_mode=text
    mock_case test-results-list-human "test-results list (human)" \
        test-results list Alpha

    expect_oracle_requests='length == 1 and any_path("/ado-harness/Empty/_apis/test/runs") and qpair("api-version=7.1")'
    expect_rust_requests='length == 1 and any_path("/ado-harness/Empty/_apis/test/runs") and qpair("api-version=7.1")'
    mock_case test-results-list-empty "test-results list (empty project)" \
        test-results list Empty --json

    envelope_rule='§8: the frozen empty list prints a leading blank line and its fixed-pad header; this build renders the content-width header and rule'
    stdout_mode=text
    mock_case test-results-list-empty-human "test-results list (empty project, human)" \
        test-results list Empty

    expect_oracle_requests='length == 1 and any_path("/ado-harness/Alpha/_apis/test/runs") and qpair("%24top=1") and qpair("api-version=7.1")'
    expect_rust_requests='length == 1 and any_path("/ado-harness/Alpha/_apis/test/runs") and qpair("%24top=1") and qpair("api-version=7.1")'
    mock_case test-results-list-top "test-results list --top 1" \
        test-results list Alpha --top 1 --json

    expect_oracle_requests='length == 1 and any_path("/ado-harness/Alpha/_apis/test/runs") and qpair("%24top=0") and qpair("api-version=7.1")'
    expect_rust_requests='length == 1 and any_path("/ado-harness/Alpha/_apis/test/runs") and qpair("%24top=0") and qpair("api-version=7.1")'
    mock_case test-results-list-top-zero "test-results list --top 0 (zero is a present option)" \
        test-results list Alpha --top 0 --json

    expect_oracle_requests='length == 1 and any_path("/ado-harness/Alpha/_apis/test/runs") and qpair("%24top=-1") and qpair("api-version=7.1")'
    expect_rust_requests='length == 1 and any_path("/ado-harness/Alpha/_apis/test/runs") and qpair("%24top=-1") and qpair("api-version=7.1")'
    mock_case test-results-list-top-negative "test-results list --top -1 (the parser takes a negative)" \
        test-results list Alpha --top -1 --json

    # Ruling 4(a): the oracle refuses the hyphen-declared option (exit 1, help on
    # stdout, no request); this build accepts it and sends the module's `buildIds`.
    status_rule='Ruling 4(a): the frozen --build-id is unreachable (invalid option, exit 1); this build accepts it as its help advertises'
    expect_statuses='1 0'
    envelope_rule='D5: the oracle prints the command help on stdout before its usage error; this build writes clap’s message to stderr alone'
    rest_rule='Ruling 4(a): the oracle sends no request for the refused flag; this build sends the filtered read'
    expect_oracle_requests='length == 0'
    expect_rust_requests='length == 1 and any_path("/ado-harness/Alpha/_apis/test/runs") and qpair("buildIds=42") and qpair("api-version=7.1")'
    mock_case test-results-list-build-id "test-results list --build-id 42 (repaired)" \
        test-results list Alpha --build-id 42 --json

    status_rule='Ruling 4(a): the frozen --min-last-updated is unreachable (invalid option, exit 1); this build accepts it as its help advertises'
    expect_statuses='1 0'
    envelope_rule='D5: the oracle prints the command help on stdout before its usage error; this build writes clap’s message to stderr alone'
    rest_rule='Ruling 4(a): the oracle sends no request for the refused flag; this build sends the filtered read'
    expect_oracle_requests='length == 0'
    expect_rust_requests='length == 1 and any_path("/ado-harness/Alpha/_apis/test/runs") and qpair("minLastUpdatedDate=2026-01-01") and qpair("api-version=7.1")'
    mock_case test-results-list-min-last-updated "test-results list --min-last-updated (repaired)" \
        test-results list Alpha --min-last-updated 2026-01-01 --json

    # D41's class: a 200 whose body has no `value` key is a silent exit 0 in the
    # oracle (the `error ->` clause's no-op formatter); this build wraps the body.
    envelope_rule='D41: the frozen list body without a `value` key exits 0 with no output; this build wraps the whole body as the single item'
    expect_statuses='0 0'
    mock_case test-results-list-novalue "test-results list (a body without value)" \
        test-results list NoValue --json

    case_org=missing
    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with inspect/2 (C12: the list-error row)'
    expect_oracle_requests='length == 1 and any_path("/missing/Alpha/_apis/test/runs") and qpair("api-version=7.1")'
    expect_rust_requests='length == 1 and any_path("/missing/Alpha/_apis/test/runs") and qpair("api-version=7.1")'
    mock_case test-results-list-404 "test-results list (404)" \
        test-results list Alpha --json

    case_org=broken
    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with inspect/2 (C12: the list-error row)'
    expect_oracle_requests='length == 1 and any_path("/broken/Alpha/_apis/test/runs") and qpair("api-version=7.1")'
    expect_rust_requests='length == 1 and any_path("/broken/Alpha/_apis/test/runs") and qpair("api-version=7.1")'
    mock_case test-results-list-500 "test-results list (500)" \
        test-results list Alpha --json

    case_org=missing
    envelope_rule='D4: the oracle prints its human error line on stdout; this build writes the labelled line to stderr alone'
    stdout_mode=text
    mock_case test-results-list-404-human "test-results list (404, human)" \
        test-results list Alpha

    case_org=broken
    envelope_rule='D4: the oracle prints its human error line on stdout; this build writes the labelled line to stderr alone'
    stdout_mode=text
    mock_case test-results-list-500-human "test-results list (500, human)" \
        test-results list Alpha

    case_org=ado-harness
    # D22: the frozen list interpolates the project raw, so a space makes Finch
    # refuse the request target before anything leaves; this build escapes the
    # segment and reads the route.
    status_rule='D22: the frozen raw project makes the request target invalid (network error, no request); this build escapes the segment and reads it'
    expect_statuses='1 0'
    envelope_rule='D22: the oracle fails before the wire with an invalid request target; this build reads the escaped path'
    rest_rule='D22: the oracle sends nothing; this build sends the escaped path'
    expect_oracle_requests='length == 0'
    expect_rust_requests='length == 1 and any_path("/ado-harness/Alpha%20Beta/_apis/test/runs") and qpair("api-version=7.1")'
    mock_case test-results-list-space-project "test-results list (a space in the project)" \
        test-results list "Alpha Beta" --json

    envelope_rule='D5: the oracle prints the command help on stdout before its usage error; this build writes clap’s message to stderr alone'
    stdout_mode=text
    mock_case test-results-list-no-project "test-results list (no project)" \
        test-results list --json

    expect_oracle_requests='length == 1 and any_path("/ado-harness/Alpha/_apis/test/runs/42") and qpair("api-version=7.1") and (.[0].query | split("&") | length == 1)'
    expect_rust_requests='length == 1 and any_path("/ado-harness/Alpha/_apis/test/runs/42") and qpair("api-version=7.1") and (.[0].query | split("&") | length == 1)'
    mock_case test-results-show "test-results show" \
        test-results show Alpha 42 --json

    expect_oracle_requests='length == 1 and any_path("/ado-harness/Alpha/_apis/test/runs/42")'
    expect_rust_requests='length == 1 and any_path("/ado-harness/Alpha/_apis/test/runs/42")'
    stdout_mode=text
    mock_case test-results-show-human "test-results show (human)" \
        test-results show Alpha 42

    expect_oracle_requests='length == 1 and any_path("/ado-harness/Alpha/_apis/test/runs/43")'
    expect_rust_requests='length == 1 and any_path("/ado-harness/Alpha/_apis/test/runs/43")'
    mock_case test-results-show-minimal "test-results show (a bare run)" \
        test-results show Alpha 43 --json

    expect_oracle_requests='length == 1 and any_path("/ado-harness/Alpha/_apis/test/runs/43")'
    expect_rust_requests='length == 1 and any_path("/ado-harness/Alpha/_apis/test/runs/43")'
    stdout_mode=text
    mock_case test-results-show-minimal-human "test-results show (a bare run, human)" \
        test-results show Alpha 43

    expect_oracle_requests='length == 1 and any_path("/ado-harness/Alpha/_apis/test/runs/44")'
    expect_rust_requests='length == 1 and any_path("/ado-harness/Alpha/_apis/test/runs/44")'
    stdout_mode=text
    mock_case test-results-show-state-stats-human "test-results show (state labels and a nil count, human)" \
        test-results show Alpha 44

    expect_oracle_requests='length == 1 and any_path("/ado-harness/Alpha/_apis/test/runs/44")'
    expect_rust_requests='length == 1 and any_path("/ado-harness/Alpha/_apis/test/runs/44")'
    mock_case test-results-show-state-stats "test-results show (state labels and a nil count)" \
        test-results show Alpha 44 --json

    case_org=missing
    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with inspect/2'
    expect_oracle_requests='length == 1 and any_path("/missing/Alpha/_apis/test/runs/42")'
    expect_rust_requests='length == 1 and any_path("/missing/Alpha/_apis/test/runs/42")'
    mock_case test-results-show-404 "test-results show (404)" \
        test-results show Alpha 42 --json

    case_org=broken
    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with inspect/2'
    expect_oracle_requests='length == 1 and any_path("/broken/Alpha/_apis/test/runs/42")'
    expect_rust_requests='length == 1 and any_path("/broken/Alpha/_apis/test/runs/42")'
    mock_case test-results-show-500 "test-results show (500)" \
        test-results show Alpha 42 --json

    case_org=ado-harness
    envelope_rule='D5: the oracle prints the command help on stdout before its usage error; this build writes clap’s message to stderr alone'
    stdout_mode=text
    mock_case test-results-show-no-id "test-results show (no run_id)" \
        test-results show Alpha

    envelope_rule='D5: the oracle prints the command help on stdout before its usage error; this build writes clap’s message to stderr alone'
    stdout_mode=text
    mock_case test-results-show-bad-id "test-results show (a non-integer run_id)" \
        test-results show Alpha not-an-integer

    rest_rule="$tr_upload_rule"
    rest_norm="$tr_upload_norm"
    envelope_rule='§8: the frozen publish prints its document followed by the halt_success "Done." marker; this build writes the document alone'
    expect_oracle_requests='length == 3 and (.[0].method == "POST") and any_path("/ado-harness/Alpha/_apis/test/runs") and qpair("api-version=7.1") and (.[2].method == "POST") and (.[2].path | endswith("/attachments")) and (.[2].query | contains("api-version=7.1-preview.1")) and (.[2].query | contains("?Content-Type=")) and (.[2].body | startswith("\"<?xml"))'
    expect_rust_requests='length == 3 and (.[0].method == "POST") and any_path("/ado-harness/Alpha/_apis/test/runs") and qpair("api-version=7.1") and (.[2].method == "POST") and (.[2].path | endswith("/attachments")) and ((.[2].query | split("&") | sort) == ["api-version=7.1-preview.1","fileName=results.xml"]) and (.[2].body | startswith("<?xml"))'
    mock_case test-results-publish "test-results publish" \
        test-results publish Alpha --name 'Nightly Regression' --file "$work/results.xml" --json

    rest_rule="$tr_upload_rule"
    rest_norm="$tr_upload_norm"
    expect_oracle_requests='length == 3 and (.[2].path | endswith("/attachments")) and (.[2].query | contains("?Content-Type=")) and (.[2].body | startswith("\"<?xml"))'
    expect_rust_requests='length == 3 and (.[2].path | endswith("/attachments")) and ((.[2].query | split("&") | sort) == ["api-version=7.1-preview.1","fileName=results.xml"]) and (.[2].body | startswith("<?xml"))'
    stdout_mode=text
    mock_case test-results-publish-human "test-results publish (human)" \
        test-results publish Alpha --name 'Nightly Regression' --file "$work/results.xml"

    rest_rule="$tr_upload_rule"
    rest_norm="$tr_upload_norm"
    envelope_rule='§8: the frozen publish prints its document followed by the halt_success "Done." marker; this build writes the document alone'
    expect_oracle_requests='length == 3 and (.[2].query | contains("fileName=results.xml"))'
    expect_rust_requests='length == 3 and (.[2].query | contains("fileName=results.xml")) and (.[2].query | contains("results.xml?") | not)'
    mock_case test-results-publish-nested-file "test-results publish (a nested --file)" \
        test-results publish Alpha --name 'Nightly Regression' --file "$work/nested/results.xml" --json

    rest_rule="$tr_upload_rule"
    rest_norm="$tr_upload_norm"
    envelope_rule='§8: the frozen publish prints its document followed by the halt_success "Done." marker; this build writes the document alone'
    expect_oracle_requests='length == 3 and any_path("/ado-harness/EmptyName/_apis/test/runs") and (.[2].method == "POST") and (.[2].path | endswith("/attachments")) and (.[2].query | contains("api-version=7.1-preview.1")) and (.[2].query | contains("?Content-Type="))'
    expect_rust_requests='length == 3 and any_path("/ado-harness/EmptyName/_apis/test/runs") and (.[2].method == "POST") and (.[2].path | endswith("/attachments")) and ((.[2].query | split("&") | sort) == ["api-version=7.1-preview.1","fileName=results.xml"])'
    mock_case test-results-publish-empty-name "test-results publish (a present empty --name)" \
        test-results publish EmptyName --name '' --file "$work/results.xml" --json

    # Ruling 4(a)'s third repair: the oracle refuses --build-id and sends nothing;
    # this build links the run to the build its help documents.
    status_rule='Ruling 4(a): the frozen publish --build-id is unreachable (invalid option, exit 1); this build accepts it and links the build'
    expect_statuses='1 0'
    envelope_rule='D5: the oracle prints the command help on stdout before its usage error; this build writes clap’s message to stderr alone'
    rest_rule='Ruling 4(a): the oracle sends no request for the refused flag; this build sends the build-linked chain'
    expect_oracle_requests='length == 0'
    expect_rust_requests='length == 3 and any_path("/ado-harness/BuildLinked/_apis/test/runs") and any_body("\"build\":{\"id\":42}") and (.[2].query | contains("fileName=results.xml"))'
    mock_case test-results-publish-build-id "test-results publish --build-id 42 (repaired)" \
        test-results publish BuildLinked --name 'Nightly Regression' --file "$work/results.xml" --build-id 42 --json

    rest_rule="$tr_upload_rule"
    rest_norm="$tr_upload_norm"
    envelope_rule='§8: the frozen publish prints its document followed by the halt_success "Done." marker; this build writes the document alone'
    expect_oracle_requests='length == 3 and any_path("/ado-harness/FailPatch/_apis/test/runs/503") and (.[2].method == "POST") and (.[2].path | endswith("/attachments")) and (.[2].query | contains("api-version=7.1-preview.1")) and (.[2].query | contains("?Content-Type="))'
    expect_rust_requests='length == 3 and any_path("/ado-harness/FailPatch/_apis/test/runs/503") and (.[2].method == "POST") and (.[2].path | endswith("/attachments")) and ((.[2].query | split("&") | sort) == ["api-version=7.1-preview.1","fileName=results.xml"])'
    mock_case test-results-publish-patch-500 "test-results publish (the completion PATCH fails)" \
        test-results publish FailPatch --name 'Nightly Regression' --file "$work/results.xml" --json

    rest_rule="$tr_upload_rule"
    rest_norm="$tr_upload_norm"
    envelope_rule='D4: the frozen publish writes its `xx  Publish failed:` block on stdout with no envelope under --json; this build emits the classified error envelope'
    expect_oracle_requests='length == 3 and any_path("/ado-harness/FailAttach/_apis/test/runs/504") and (.[2].method == "POST") and (.[2].path | endswith("/attachments")) and (.[2].query | contains("api-version=7.1-preview.1")) and (.[2].query | contains("?Content-Type="))'
    expect_rust_requests='length == 3 and any_path("/ado-harness/FailAttach/_apis/test/runs/504") and (.[2].method == "POST") and (.[2].path | endswith("/attachments")) and ((.[2].query | split("&") | sort) == ["api-version=7.1-preview.1","fileName=results.xml"])'
    mock_case test-results-publish-attach-500 "test-results publish (the upload fails)" \
        test-results publish FailAttach --name 'Nightly Regression' --file "$work/results.xml" --json

    case_org=missing
    envelope_rule='D4: the frozen publish writes its `xx  Publish failed:` block on stdout with no envelope under --json; this build emits the classified error envelope'
    expect_oracle_requests='length == 1 and (.[0].method == "POST") and any_path("/missing/Alpha/_apis/test/runs")'
    expect_rust_requests='length == 1 and (.[0].method == "POST") and any_path("/missing/Alpha/_apis/test/runs")'
    mock_case test-results-publish-create-404 "test-results publish (the create 404s)" \
        test-results publish Alpha --name 'Nightly Regression' --file "$work/results.xml" --json

    case_org=broken
    envelope_rule='D4: the frozen publish writes its `xx  Publish failed:` block on stdout with no envelope under --json; this build emits the classified error envelope'
    expect_oracle_requests='length == 1 and (.[0].method == "POST") and any_path("/broken/Alpha/_apis/test/runs")'
    expect_rust_requests='length == 1 and (.[0].method == "POST") and any_path("/broken/Alpha/_apis/test/runs")'
    mock_case test-results-publish-create-500 "test-results publish (the create 500s)" \
        test-results publish Alpha --name 'Nightly Regression' --file "$work/results.xml" --json

    case_org=ado-harness
    # The non-UTF-8 file: the frozen `JSON.encode!` raises after the PATCH and the
    # rescue exits 0 with two requests; this build sends the bytes as the third.
    status_rule='D34: the frozen JSON encode on a non-UTF-8 file raises after its PATCH and the rescue exits 0; this build uploads the bytes'
    expect_statuses='0 0'
    envelope_rule='D34: the oracle’s encode crash leaves no output; this build reports the upload'
    rest_rule='D34: the oracle’s mid-chain crash sends two requests; this build sends the upload as the third'
    expect_oracle_requests='length == 2 and any_path("/ado-harness/BinaryUpload/_apis/test/runs")'
    expect_rust_requests='length == 3 and any_path("/ado-harness/BinaryUpload/_apis/test/runs/506/attachments") and (.[2].query | contains("fileName=results.bin")) and any_body("binary")'
    mock_case test-results-publish-binary-file "test-results publish (a non-UTF-8 file)" \
        test-results publish BinaryUpload --name 'Nightly Regression' --file "$work/results.bin" --json

    status_rule='D4: the frozen read-failure path writes the module wording to stderr with no envelope under --json; this build emits the classified error envelope'
    envelope_rule='D4: the frozen read-failure path writes the module wording to stderr with no envelope under --json; this build emits the classified error envelope'
    expect_oracle_requests='length == 0'
    expect_rust_requests='length == 0'
    mock_case test-results-publish-file-missing "test-results publish (a missing --file)" \
        test-results publish Alpha --name 'Nightly Regression' --file "$work/nope.xml" --json

    status_rule='D34: a missing required option is a silent exit 0 in the oracle (the module’s Map.fetch! crash); this build is a loud usage error'
    expect_statuses='0 1'
    stdout_mode=text
    mock_case test-results-publish-no-name "test-results publish (no --name)" \
        test-results publish Alpha --file "$work/results.xml" --json

    status_rule='D34: a missing required option is a silent exit 0 in the oracle (the module’s Map.fetch! crash); this build is a loud usage error'
    expect_statuses='0 1'
    stdout_mode=text
    mock_case test-results-publish-no-file "test-results publish (no --file)" \
        test-results publish Alpha --name 'Nightly Regression' --json

    envelope_rule='D5: the oracle prints the command help on stdout before its usage error; this build writes clap’s message to stderr alone'
    stdout_mode=text
    mock_case test-results-publish-no-project "test-results publish (no project)" \
        test-results publish --name 'Nightly Regression' --file "$work/results.xml" --json

    # ── security ──
    #
    # The wave's second guard family. The typed flag's absence is a **loud**
    # refusal on both sides — exit 1, stdout empty, the same sentence on stderr —
    # so those cases run in text mode even under --json (this build emits no
    # document by D32's rule, and the oracle emits none either) and compare equal.
    # Every error path is D4 (the oracle's stderr-only prose against this build's
    # envelope), the two success writes are D33, and the one D34 row is this
    # area's own: a matching project entry without an `id` crashes the frozen
    # `%{"id" => id}` match into a silent exit 0.

    expect_oracle_requests='length == 0'
    expect_rust_requests='length == 0'
    stdout_mode=text
    mock_case security-grant-guard "security grant (no flag)" \
        security grant Alpha

    expect_oracle_requests='length == 0'
    expect_rust_requests='length == 0'
    stdout_mode=text
    mock_case security-revoke-guard "security revoke (no flag)" \
        security revoke Alpha

    # The invocation carries --json and both sides still print nothing, which is
    # why the mode is text: the case asserts stdout empty and exit 1, not a
    # document (D32's no-envelope half, matched by D32's own refusal shape).
    expect_oracle_requests='length == 0'
    expect_rust_requests='length == 0'
    stdout_mode=text
    mock_case security-grant-guard-json "security grant (no flag, --json)" \
        security grant Alpha --json

    security_chain='length == 3 and (.[0].path == "/sec/_apis/projects") and qpair("api-version=7.1") and (.[1].path == "/sec/_apis/connectionData") and qpair("api-version=7.1-preview.1") and (.[2].method == "POST") and (.[2].path | endswith("/_apis/accesscontrolentries/b7e84409-6553-448a-bbb2-af228e07cbeb")) and (.[2].query | contains("api-version=7.1"))'

    envelope_rule='D33: the frozen write paths print their human sentence under --json; this build emits the message envelope'
    expect_oracle_requests="$security_chain and any_body(\"\\\"token\\\":\\\"6a1f8f6e-2b8d-4b9e-9d2a-1c3f5e7a9b0c\\\"\") and any_body(\"\\\"merge\\\":true\") and any_body(\"\\\"allow\\\":8\") and any_body(\"\\\"extendedInfo\\\":{}\")"
    expect_rust_requests="$security_chain and any_body(\"\\\"token\\\":\\\"6a1f8f6e-2b8d-4b9e-9d2a-1c3f5e7a9b0c\\\"\") and any_body(\"\\\"merge\\\":true\") and any_body(\"\\\"allow\\\":8\") and any_body(\"\\\"extendedInfo\\\":{}\")"
    case_org=sec
    mock_case security-grant "security grant" \
        security grant Alpha --yes-this-mutates-secret-read --json

    stdout_mode=text
    expect_oracle_requests="$security_chain and any_body(\"\\\"token\\\":\\\"6a1f8f6e-2b8d-4b9e-9d2a-1c3f5e7a9b0c\\\"\") and any_body(\"\\\"merge\\\":true\")"
    expect_rust_requests="$security_chain and any_body(\"\\\"token\\\":\\\"6a1f8f6e-2b8d-4b9e-9d2a-1c3f5e7a9b0c\\\"\") and any_body(\"\\\"merge\\\":true\")"
    case_org=sec
    mock_case security-grant-human "security grant (human)" \
        security grant Alpha --yes-this-mutates-secret-read

    envelope_rule='D33: the frozen write paths print their human sentence under --json; this build emits the message envelope'
    expect_oracle_requests="$security_chain and any_body(\"\\\"token\\\":\\\"6a1f8f6e-2b8d-4b9e-9d2a-1c3f5e7a9b0c\\\"\") and any_body(\"\\\"merge\\\":false\") and any_body(\"\\\"allow\\\":0\")"
    expect_rust_requests="$security_chain and any_body(\"\\\"token\\\":\\\"6a1f8f6e-2b8d-4b9e-9d2a-1c3f5e7a9b0c\\\"\") and any_body(\"\\\"merge\\\":false\") and any_body(\"\\\"allow\\\":0\")"
    case_org=sec
    mock_case security-revoke "security revoke" \
        security revoke Alpha --yes-this-mutates-secret-read --json

    stdout_mode=text
    expect_oracle_requests='length == 3 and (.[0].path == "/sec/_apis/projects") and (.[2].method == "POST")'
    expect_rust_requests='length == 3 and (.[0].path == "/sec/_apis/projects") and (.[2].method == "POST")'
    case_org=sec
    mock_case security-revoke-human "security revoke (human)" \
        security revoke Alpha --yes-this-mutates-secret-read

    # The UUID short-circuit: the lookup is absent, and the filter names its
    # absence — a build that always looked the project up would still pass a
    # filter that only described the two requests it does send.
    envelope_rule='D33: the frozen write paths print their human sentence under --json; this build emits the message envelope'
    expect_oracle_requests='length == 2 and (any_path("/_apis/projects") | not) and (.[0].path == "/sec/_apis/connectionData") and qpair("api-version=7.1-preview.1") and (.[1].method == "POST") and any_body("\"token\":\"11111111-2222-3333-4444-555555555555\"") and any_body("\"merge\":true")'
    expect_rust_requests='length == 2 and (any_path("/_apis/projects") | not) and (.[0].path == "/sec/_apis/connectionData") and qpair("api-version=7.1-preview.1") and (.[1].method == "POST") and any_body("\"token\":\"11111111-2222-3333-4444-555555555555\"") and any_body("\"merge\":true")'
    case_org=sec
    mock_case security-grant-uuid "security grant (a UUID project)" \
        security grant 11111111-2222-3333-4444-555555555555 --yes-this-mutates-secret-read --json

    envelope_rule='D33: the frozen write paths print their human sentence under --json; this build emits the message envelope'
    expect_oracle_requests='length == 2 and (any_path("/_apis/projects") | not) and (.[0].path == "/sec-revoke/_apis/connectionData") and (.[1].method == "POST") and any_body("\"token\":\"11111111-2222-3333-4444-555555555555\"") and any_body("\"merge\":false") and any_body("\"allow\":0")'
    expect_rust_requests='length == 2 and (any_path("/_apis/projects") | not) and (.[0].path == "/sec-revoke/_apis/connectionData") and (.[1].method == "POST") and any_body("\"token\":\"11111111-2222-3333-4444-555555555555\"") and any_body("\"merge\":false") and any_body("\"allow\":0")'
    case_org=sec-revoke
    mock_case security-revoke-uuid "security revoke (a UUID project)" \
        security revoke 11111111-2222-3333-4444-555555555555 --yes-this-mutates-secret-read --json

    expect_oracle_requests='length == 1 and (.[0].path == "/sec/_apis/projects") and qpair("api-version=7.1")'
    expect_rust_requests='length == 1 and (.[0].path == "/sec/_apis/projects") and qpair("api-version=7.1")'
    case_org=sec
    envelope_rule='D4: the frozen CLI writes the module wording to stderr with no envelope under --json where this build emits the error envelope'
    mock_case security-grant-not-found "security grant (a name the list does not carry)" \
        security grant Nope --yes-this-mutates-secret-read --json

    stdout_mode=text
    expect_oracle_requests='length == 1 and (.[0].path == "/sec/_apis/projects")'
    expect_rust_requests='length == 1 and (.[0].path == "/sec/_apis/projects")'
    case_org=sec
    mock_case security-grant-not-found-human "security grant (a name the list does not carry, human)" \
        security grant Nope --yes-this-mutates-secret-read

    envelope_rule='D4: the frozen CLI writes the module wording to stderr with no envelope under --json where this build emits the error envelope'
    expect_oracle_requests='length == 0'
    expect_rust_requests='length == 0'
    mock_case security-grant-empty-project "security grant (an empty project)" \
        security grant '' --yes-this-mutates-secret-read --json

    envelope_rule='D4: the frozen CLI writes the module wording to stderr with no envelope under --json where this build emits the error envelope'
    expect_oracle_requests='length == 0'
    expect_rust_requests='length == 0'
    mock_case security-grant-permission "security grant --permission ViewLibrary" \
        security grant Alpha --yes-this-mutates-secret-read --permission ViewLibrary --json

    stdout_mode=text
    expect_oracle_requests='length == 0'
    expect_rust_requests='length == 0'
    mock_case security-grant-permission-human "security grant --permission ViewLibrary (human)" \
        security grant Alpha --yes-this-mutates-secret-read --permission ViewLibrary

    envelope_rule='D4: the frozen CLI writes the module wording to stderr with no envelope under --json where this build emits the error envelope'
    expect_oracle_requests='length == 0'
    expect_rust_requests='length == 0'
    mock_case security-grant-permission-empty "security grant --permission ''" \
        security grant Alpha --yes-this-mutates-secret-read --permission '' --json

    envelope_rule='D4: the frozen CLI writes the module wording to stderr with no envelope under --json where this build emits the error envelope'
    expect_oracle_requests='length == 1 and (.[0].path == "/sec-novalue/_apis/projects")'
    expect_rust_requests='length == 1 and (.[0].path == "/sec-novalue/_apis/projects")'
    case_org=sec-novalue
    mock_case security-grant-novalue "security grant (a projects body without value)" \
        security grant Alpha --yes-this-mutates-secret-read --json

    status_rule='D34: the frozen `%{"id" => id}` match crashes on an entry without an id and the rescue exits 0; this build fails loudly after the lookup'
    expect_statuses='0 1'
    envelope_rule='D34: the frozen crash leaves no output; this build emits the lookup failure'
    expect_oracle_requests='length == 1 and (.[0].path == "/sec-noid/_apis/projects")'
    expect_rust_requests='length == 1 and (.[0].path == "/sec-noid/_apis/projects")'
    case_org=sec-noid
    mock_case security-grant-noid "security grant (a matching entry with no id)" \
        security grant Alpha --yes-this-mutates-secret-read --json

    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with the step name and inspect/2'
    expect_oracle_requests='length == 1 and (.[0].path == "/sec-proj-broken/_apis/projects")'
    expect_rust_requests='length == 1 and (.[0].path == "/sec-proj-broken/_apis/projects")'
    case_org=sec-proj-broken
    mock_case security-grant-proj-500 "security grant (the projects lookup 500s)" \
        security grant Alpha --yes-this-mutates-secret-read --json

    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with the step name and inspect/2 (C12: the lookup-error row)'
    expect_oracle_requests='length == 1 and (.[0].path == "/sec-proj-missing/_apis/projects")'
    expect_rust_requests='length == 1 and (.[0].path == "/sec-proj-missing/_apis/projects")'
    case_org=sec-proj-missing
    mock_case security-grant-proj-404 "security grant (the projects lookup 404s)" \
        security grant Alpha --yes-this-mutates-secret-read --json

    envelope_rule='D4: the frozen CLI writes the MSA refusal to stderr with no envelope under --json where this build emits the error envelope'
    expect_oracle_requests='length == 2 and (.[0].path == "/sec-msa/_apis/projects") and (.[1].path == "/sec-msa/_apis/connectionData") and (any_body("msa.") | not)'
    expect_rust_requests='length == 2 and (.[0].path == "/sec-msa/_apis/projects") and (.[1].path == "/sec-msa/_apis/connectionData") and (any_body("msa.") | not)'
    case_org=sec-msa
    mock_case security-grant-msa "security grant (an MSA descriptor)" \
        security grant Alpha --yes-this-mutates-secret-read --json

    envelope_rule='D4: the frozen CLI writes the missing-descriptor refusal to stderr with no envelope under --json where this build emits the error envelope'
    expect_oracle_requests='length == 2 and (.[0].path == "/sec-nodesc/_apis/projects") and (.[1].path == "/sec-nodesc/_apis/connectionData")'
    expect_rust_requests='length == 2 and (.[0].path == "/sec-nodesc/_apis/projects") and (.[1].path == "/sec-nodesc/_apis/connectionData")'
    case_org=sec-nodesc
    mock_case security-grant-nodesc "security grant (no subjectDescriptor)" \
        security grant Alpha --yes-this-mutates-secret-read --json

    envelope_rule='D4: the frozen CLI writes the missing-descriptor refusal to stderr with no envelope under --json where this build emits the error envelope'
    expect_oracle_requests='length == 2 and (.[0].path == "/sec-nodesc-empty/_apis/projects") and (.[1].path == "/sec-nodesc-empty/_apis/connectionData")'
    expect_rust_requests='length == 2 and (.[0].path == "/sec-nodesc-empty/_apis/projects") and (.[1].path == "/sec-nodesc-empty/_apis/connectionData")'
    case_org=sec-nodesc-empty
    mock_case security-grant-nodesc-empty "security grant (an empty subjectDescriptor)" \
        security grant Alpha --yes-this-mutates-secret-read --json

    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with the step name and inspect/2'
    expect_oracle_requests='length == 2 and (.[0].path == "/sec-conn-broken/_apis/projects") and (.[1].path == "/sec-conn-broken/_apis/connectionData") and (any_path("accesscontrolentries") | not)'
    expect_rust_requests='length == 2 and (.[0].path == "/sec-conn-broken/_apis/projects") and (.[1].path == "/sec-conn-broken/_apis/connectionData") and (any_path("accesscontrolentries") | not)'
    case_org=sec-conn-broken
    mock_case security-grant-conn-500 "security grant (the descriptor fetch 500s)" \
        security grant Alpha --yes-this-mutates-secret-read --json

    sec_acl='any_path("/_apis/accesscontrolentries/b7e84409-6553-448a-bbb2-af228e07cbeb")'

    envelope_rule='D24: the module’s three-cause prose carries the raw upstream bytes here, where the oracle interpolates inspect/2 of the decoded map'
    expect_oracle_requests='length == 2 and (any_path("/_apis/projects") | not) and (.[0].path == "/sec-reject/_apis/connectionData") and (.[1].method == "POST") and '"$sec_acl"' and any_body("\"merge\":true")'
    expect_rust_requests='length == 2 and (any_path("/_apis/projects") | not) and (.[0].path == "/sec-reject/_apis/connectionData") and (.[1].method == "POST") and '"$sec_acl"' and any_body("\"merge\":true")'
    case_org=sec-reject
    mock_case security-grant-403 "security grant (a 403)" \
        security grant 11111111-2222-3333-4444-555555555555 --yes-this-mutates-secret-read --json

    envelope_rule='D24: the module’s three-cause prose names the revoke and carries the raw upstream bytes'
    expect_oracle_requests='length == 2 and (any_path("/_apis/projects") | not) and (.[0].path == "/sec-revoke-reject/_apis/connectionData") and (.[1].method == "POST") and '"$sec_acl"' and any_body("\"merge\":false")'
    expect_rust_requests='length == 2 and (any_path("/_apis/projects") | not) and (.[0].path == "/sec-revoke-reject/_apis/connectionData") and (.[1].method == "POST") and '"$sec_acl"' and any_body("\"merge\":false")'
    case_org=sec-revoke-reject
    mock_case security-revoke-403 "security revoke (a 403)" \
        security revoke 11111111-2222-3333-4444-555555555555 --yes-this-mutates-secret-read --json

    envelope_rule='D24: the module’s three-cause prose carries the raw upstream bytes here, where the oracle interpolates inspect/2 of the decoded map'
    expect_oracle_requests='length == 2 and (any_path("/_apis/projects") | not) and (.[0].path == "/sec-auth/_apis/connectionData") and (.[1].method == "POST") and '"$sec_acl"''
    expect_rust_requests='length == 2 and (any_path("/_apis/projects") | not) and (.[0].path == "/sec-auth/_apis/connectionData") and (.[1].method == "POST") and '"$sec_acl"''
    case_org=sec-auth
    mock_case security-grant-401 "security grant (a 401)" \
        security grant 11111111-2222-3333-4444-555555555555 --yes-this-mutates-secret-read --json

    envelope_rule='D24: the module’s three-cause prose carries the raw upstream bytes here, where the oracle interpolates inspect/2 of the decoded map'
    expect_oracle_requests='length == 2 and (any_path("/_apis/projects") | not) and (.[0].path == "/sec-bad/_apis/connectionData") and (.[1].method == "POST") and '"$sec_acl"''
    expect_rust_requests='length == 2 and (any_path("/_apis/projects") | not) and (.[0].path == "/sec-bad/_apis/connectionData") and (.[1].method == "POST") and '"$sec_acl"''
    case_org=sec-bad
    mock_case security-grant-400 "security grant (a 400)" \
        security grant 11111111-2222-3333-4444-555555555555 --yes-this-mutates-secret-read --json

    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with the step name and inspect/2'
    expect_oracle_requests='length == 2 and (any_path("/_apis/projects") | not) and (.[0].path == "/sec-acl-broken/_apis/connectionData") and (.[1].method == "POST") and '"$sec_acl"''
    expect_rust_requests='length == 2 and (any_path("/_apis/projects") | not) and (.[0].path == "/sec-acl-broken/_apis/connectionData") and (.[1].method == "POST") and '"$sec_acl"''
    case_org=sec-acl-broken
    mock_case security-grant-acl-500 "security grant (the write 500s)" \
        security grant 11111111-2222-3333-4444-555555555555 --yes-this-mutates-secret-read --json

    # D43's spelling family on the guard. `=true` proceeds in the oracle; this
    # build’s clap refuses the =value form, so the statuses and the chains differ.
    status_rule='D43: the frozen parser takes --yes-this-mutates-secret-read=true and proceeds; this build’s clap refuses the =value spelling'
    expect_statuses='0 1'
    envelope_rule='D5: the oracle prints its human sentence and proceeds, this build writes clap’s usage error to stderr alone'
    rest_rule='D43: the oracle proceeds with the flag set and walks the chain; this build refuses the spelling and sends nothing'
    expect_oracle_requests="$security_chain and any_body(\"\\\"merge\\\":true\")"
    expect_rust_requests='length == 0'
    stdout_mode=text
    case_org=sec
    mock_case security-grant-flag-eq-true "security grant --yes-this-mutates-secret-read=true (the unported spelling)" \
        security grant Alpha --yes-this-mutates-secret-read=true

    # Both refuse with exit 1 and empty stdout, so the case compares equal; the
    # mechanism differs (the oracle parses the value and its own guard refuses,
    # clap rejects the spelling) and the integration suite pins this side's.
    expect_oracle_requests='length == 0'
    expect_rust_requests='length == 0'
    stdout_mode=text
    mock_case security-grant-flag-eq-false "security grant --yes-this-mutates-secret-read=false (the unported spelling)" \
        security grant Alpha --yes-this-mutates-secret-read=false --json

    expect_oracle_requests='length == 0'
    expect_rust_requests='length == 0'
    stdout_mode=text
    mock_case security-grant-flag-no "security grant --no-yes-this-mutates-secret-read (the unported spelling)" \
        security grant Alpha --no-yes-this-mutates-secret-read --json

    status_rule='D43: the last spelling wins in the frozen parser (--flag=false then the bare flag is true); this build refuses the =value spelling'
    expect_statuses='0 1'
    envelope_rule='D5: the oracle prints its human sentence and proceeds, this build writes clap’s usage error to stderr alone'
    rest_rule='D43: the last spelling wins, so the oracle walks the chain; this build refuses the first spelling and sends nothing'
    expect_oracle_requests="$security_chain and any_body(\"\\\"merge\\\":true\")"
    expect_rust_requests='length == 0'
    stdout_mode=text
    case_org=sec
    mock_case security-grant-flag-last-wins "security grant --yes-this-mutates-secret-read=false --yes-this-mutates-secret-read (last spelling wins)" \
        security grant Alpha --yes-this-mutates-secret-read=false --yes-this-mutates-secret-read

    # Only the literals true/false parse in the oracle: =1 is an `invalid option`
    # usage error there and a clap error here (D5's stdout side).
    envelope_rule='D5: the oracle prints the command help on stdout before its usage error; this build writes clap’s message to stderr alone'
    expect_oracle_requests='length == 0'
    expect_rust_requests='length == 0'
    stdout_mode=text
    mock_case security-grant-flag-eq-one "security grant --yes-this-mutates-secret-read=1 (the value is not a literal)" \
        security grant Alpha --yes-this-mutates-secret-read=1 --json

    # D17's class: the option is declared with underscores and runnable hyphenated.
    envelope_rule='D17/D5: the oracle refuses the underscore spelling (invalid option) with help on stdout; this build writes clap’s message to stderr alone'
    expect_oracle_requests='length == 0'
    expect_rust_requests='length == 0'
    stdout_mode=text
    mock_case security-grant-flag-underscore "security grant --yes_this_mutates_secret_read (the underscore spelling)" \
        security grant Alpha --yes_this_mutates_secret_read --json

    # A dash-leading project argument is an option to the frozen parser and to
    # clap: one usage error each, help on stdout there and nothing here (D5).
    envelope_rule='D5: the oracle prints the command help on stdout before its usage error; this build writes clap’s message to stderr alone'
    expect_oracle_requests='length == 0'
    expect_rust_requests='length == 0'
    stdout_mode=text
    mock_case security-grant-dash-project "security grant -Alpha (a dash-leading project)" \
        security grant -Alpha --yes-this-mutates-secret-read --json

    # The boundary: `-1` is a positional to OptionParser's negative-number rule
    # and an option to clap, so the oracle looks the name up and this build sends
    # nothing. The filter names the lookup it must send.
    envelope_rule='D5: the oracle prints the command help on stdout before its usage error; this build writes clap’s message to stderr alone'
    rest_rule='the frozen parser reads -1 as a positional (its negative-number rule) and looks it up; clap reads it as an option and sends nothing'
    expect_oracle_requests='length == 1 and (.[0].path == "/sec/_apis/projects") and qpair("api-version=7.1")'
    expect_rust_requests='length == 0'
    stdout_mode=text
    case_org=sec
    mock_case security-grant-negative-project "security grant -1 (the oracle’s negative-number rule)" \
        security grant -1 --yes-this-mutates-secret-read --json

    # `--` is the escape both parsers honour: the dash-leading project becomes
    # the positional and the guard refuses for want of the flag on both sides.
    expect_oracle_requests='length == 0'
    expect_rust_requests='length == 0'
    stdout_mode=text
    mock_case security-grant-escape "security grant -- -Alpha (the escape)" \
        security grant -- -Alpha

    envelope_rule='D5: the oracle prints the command help on stdout before its usage error; this build writes clap’s message to stderr alone'
    expect_oracle_requests='length == 0'
    expect_rust_requests='length == 0'
    stdout_mode=text
    mock_case security-no-subcommand "security (no sub-command)" \
        security --json

    envelope_rule='D5: the oracle prints the command help on stdout before its usage error; this build writes clap’s message to stderr alone'
    expect_oracle_requests='length == 0'
    expect_rust_requests='length == 0'
    stdout_mode=text
    mock_case security-grant-no-project "security grant (no project)" \
        security grant --yes-this-mutates-secret-read --json

    envelope_rule='D5: the oracle prints the command help on stdout before its usage error; this build writes clap’s message to stderr alone'
    expect_oracle_requests='length == 0'
    expect_rust_requests='length == 0'
    stdout_mode=text
    mock_case security-grant-extra "security grant (an extra positional)" \
        security grant Alpha Extra --yes-this-mutates-secret-read --json

    envelope_rule='D5: the oracle prints the command help on stdout before its usage error; this build writes clap’s message to stderr alone'
    expect_oracle_requests='length == 0'
    expect_rust_requests='length == 0'
    stdout_mode=text
    mock_case security-grant-permission-valueless "security grant (a valueless --permission)" \
        security grant Alpha --yes-this-mutates-secret-read --permission

    # ── Task 8: the ci watcher ──────────────────────────────────────────
    #
    # `ci watch` is the wave's only command whose product is a stream, and the
    # only one the harness drives with a `sequence`: the build route answers each
    # poll in turn, so both sides walk the same chain whatever their poll cadence
    # (the oracle's fixed 2000 ms; ours as documented). The stdout cases run in
    # `text` mode — the human stream after §8's colour strip — and the
    # load-bearing request chain is asserted by the direction filters. Five cases
    # carry the repaired pair (Ruling 3): failed 0/1, canceled 0/2, cancelling
    # 0/2, the log stream's terminal failure 0/1, and the `--poll-interval`
    # repair (Ruling 4(a))'s oracle 1 / rust 0.

    expect_statuses='0 0'
    expect_oracle_requests='length == 2 and (.[0].path == "/ado-harness/Watch/_apis/build/builds/201") and qpair("api-version=7.1") and (.[1].path == "/ado-harness/Watch/_apis/build/builds/201/timeline")'
    expect_rust_requests='length == 2 and (.[0].path == "/ado-harness/Watch/_apis/build/builds/201") and qpair("api-version=7.1") and (.[1].path == "/ado-harness/Watch/_apis/build/builds/201/timeline")'
    stdout_mode=text
    mock_case ci-watch-completed "ci watch (a completed build)" \
        ci watch Watch 201

    expect_statuses='0 0'
    expect_oracle_requests='length == 5 and ([.[] | select(.path == "/ado-harness/Watch/_apis/build/builds/202")] | length) == 2 and ([.[] | select(.path == "/ado-harness/Watch/_apis/build/builds/202/timeline")] | length) == 3'
    expect_rust_requests='length == 5 and ([.[] | select(.path == "/ado-harness/Watch/_apis/build/builds/202")] | length) == 2 and ([.[] | select(.path == "/ado-harness/Watch/_apis/build/builds/202/timeline")] | length) == 3'
    stdout_mode=text
    mock_case ci-watch-two-tick "ci watch (inProgress → succeeded)" \
        ci watch Watch 202

    status_rule='Ruling 3: the frozen watcher returns :ok for every terminal state, so a failed build exits 0; this build exits 1'
    expect_statuses='0 1'
    envelope_rule='Ruling 3: the final line is ✗ Build 203 failed. here, where the oracle prints ✓ Build 203 completed. after its own Build failed.'
    expect_oracle_requests='length == 2 and (.[0].path == "/ado-harness/Watch/_apis/build/builds/203") and (.[1].path == "/ado-harness/Watch/_apis/build/builds/203/timeline")'
    expect_rust_requests='length == 2 and (.[0].path == "/ado-harness/Watch/_apis/build/builds/203") and (.[1].path == "/ado-harness/Watch/_apis/build/builds/203/timeline")'
    stdout_mode=text
    mock_case ci-watch-failed "ci watch (a failed build)" \
        ci watch Watch 203

    status_rule='Ruling 3: a canceled build is the doc’s cancellation; the frozen exits 0, this build exits 2'
    expect_statuses='0 2'
    envelope_rule='Ruling 3: the final line is ✗ Build 204 canceled. here, where the oracle prints ✓ Build 204 completed.'
    expect_oracle_requests='length == 2 and (.[0].path == "/ado-harness/Watch/_apis/build/builds/204") and (.[1].path == "/ado-harness/Watch/_apis/build/builds/204/timeline")'
    expect_rust_requests='length == 2 and (.[0].path == "/ado-harness/Watch/_apis/build/builds/204") and (.[1].path == "/ado-harness/Watch/_apis/build/builds/204/timeline")'
    stdout_mode=text
    mock_case ci-watch-canceled "ci watch (a canceled build)" \
        ci watch Watch 204

    expect_statuses='0 0'
    expect_oracle_requests='length == 2 and (.[0].path == "/ado-harness/Watch/_apis/build/builds/205") and (.[1].path == "/ado-harness/Watch/_apis/build/builds/205/timeline")'
    expect_rust_requests='length == 2 and (.[0].path == "/ado-harness/Watch/_apis/build/builds/205") and (.[1].path == "/ado-harness/Watch/_apis/build/builds/205/timeline")'
    stdout_mode=text
    mock_case ci-watch-partial "ci watch (a partially succeeded build)" \
        ci watch Watch 205

    status_rule='Ruling 3: the frozen terminal `cancelling` state exits 0; this build reads a cancellation in flight as 2'
    expect_statuses='0 2'
    envelope_rule='Ruling 3: the final line is ✗ Build 206 canceled. here, where the oracle prints ✓ Build 206 completed.'
    expect_oracle_requests='length == 2 and (.[0].path == "/ado-harness/Watch/_apis/build/builds/206") and (.[1].path == "/ado-harness/Watch/_apis/build/builds/206/timeline")'
    expect_rust_requests='length == 2 and (.[0].path == "/ado-harness/Watch/_apis/build/builds/206") and (.[1].path == "/ado-harness/Watch/_apis/build/builds/206/timeline")'
    stdout_mode=text
    mock_case ci-watch-cancelling "ci watch (a cancelling build)" \
        ci watch Watch 206

    # The log stream's own contract is the `?id=N` progression: `?id=1`, then
    # `?id=3` after the two CRLF lines — only the request log can see it, and the
    # filter names both indices, both exact pairs and the five timeline fetches.
    status_rule='Ruling 3: the frozen watcher returns :ok on the terminal failed tick, so it exits 0; this build exits 1'
    expect_statuses='0 1'
    envelope_rule='Ruling 3: the final line is ✗ Build 207 failed. here, where the oracle prints ✓ Build 207 completed.'
    expect_oracle_requests='length == 10 and ([.[] | select(.path | endswith("/timeline"))] | length) == 5 and (.[3].path == "/ado-harness/Watch/_apis/build/builds/207/logs/7") and (.[3].query | split("&") | index("id=1")) != null and (.[7].path == "/ado-harness/Watch/_apis/build/builds/207/logs/7") and (.[7].query | split("&") | index("id=3")) != null'
    expect_rust_requests='length == 10 and ([.[] | select(.path | endswith("/timeline"))] | length) == 5 and (.[3].path == "/ado-harness/Watch/_apis/build/builds/207/logs/7") and (.[3].query | split("&") | index("id=1")) != null and (.[7].path == "/ado-harness/Watch/_apis/build/builds/207/logs/7") and (.[7].query | split("&") | index("id=3")) != null'
    stdout_mode=text
    mock_case ci-watch-logs "ci watch (the log stream)" \
        ci watch Watch 207

    expect_statuses='0 0'
    expect_oracle_requests='length == 3 and (.[0].path == "/ado-harness/Watch/_apis/build/builds") and qpair("%24top=1") and (.[1].path == "/ado-harness/Watch/_apis/build/builds/201") and (.[2].path == "/ado-harness/Watch/_apis/build/builds/201/timeline")'
    expect_rust_requests='length == 3 and (.[0].path == "/ado-harness/Watch/_apis/build/builds") and qpair("%24top=1") and (.[1].path == "/ado-harness/Watch/_apis/build/builds/201") and (.[2].path == "/ado-harness/Watch/_apis/build/builds/201/timeline")'
    stdout_mode=text
    mock_case ci-watch-latest "ci watch --latest" \
        ci watch Watch --latest

    expect_statuses='0 0'
    expect_oracle_requests='length == 3 and qpair("%24top=1") and qpair("definitions=7") and qpair("branchName=refs%2Fheads%2Fmain") and (.[1].path == "/ado-harness/Watch/_apis/build/builds/201")'
    expect_rust_requests='length == 3 and qpair("%24top=1") and qpair("definitions=7") and qpair("branchName=refs%2Fheads%2Fmain") and (.[1].path == "/ado-harness/Watch/_apis/build/builds/201")'
    stdout_mode=text
    mock_case ci-watch-latest-filters "ci watch --latest --definition/--branch" \
        ci watch Watch --latest --definition 7 --branch refs/heads/main

    envelope_rule='D4: the oracle prints its resolve error sentence on stdout; this build writes the classified error to stderr'
    expect_statuses='1 1'
    expect_oracle_requests='length == 1 and (.[0].path == "/ado-harness/EmptyWatch/_apis/build/builds")'
    expect_rust_requests='length == 1 and (.[0].path == "/ado-harness/EmptyWatch/_apis/build/builds")'
    stdout_mode=text
    mock_case ci-watch-latest-empty "ci watch --latest (no builds)" \
        ci watch EmptyWatch --latest

    envelope_rule='D4: the oracle prints its resolve error sentence on stdout; this build writes the classified error to stderr'
    expect_statuses='1 1'
    expect_oracle_requests='length == 0'
    expect_rust_requests='length == 0'
    stdout_mode=text
    mock_case ci-watch-no-id "ci watch (no build id)" \
        ci watch Watch

    # Ruling 4(a): the frozen option is a hyphen-declared key no spelling can
    # match, so the oracle refuses it and sends nothing; this build accepts the
    # advertised spelling and walks the whole chain. The direction is the case's
    # point: the oracle side must stay empty, the rust side must fetch.
    status_rule='Ruling 4(a): the frozen --poll-interval is unreachable and exits 1; this build accepts it as documented'
    rest_rule='Ruling 4(a): the oracle refuses the hyphen-declared option and sends nothing; this build walks the watch chain'
    envelope_rule='D5/D48: the oracle prints the command help on stdout before its invalid-option line; this build runs the watch'
    expect_statuses='1 0'
    expect_oracle_requests='length == 0'
    expect_rust_requests='length == 2 and (.[0].path == "/ado-harness/Watch/_apis/build/builds/201") and (.[1].path == "/ado-harness/Watch/_apis/build/builds/201/timeline")'
    stdout_mode=text
    mock_case ci-watch-poll-interval "ci watch --poll-interval 250 (the repair)" \
        ci watch Watch 201 --poll-interval 250

    envelope_rule='D24: the error body stays the upstream bytes here, where the oracle re-renders the decoded map with its step name'
    expect_statuses='1 1'
    expect_oracle_requests='length == 1 and (.[0].path == "/ado-harness/Watch/_apis/build/builds/404")'
    expect_rust_requests='length == 1 and (.[0].path == "/ado-harness/Watch/_apis/build/builds/404")'
    stdout_mode=text
    mock_case ci-watch-missing-build "ci watch (a 404 build)" \
        ci watch Watch 404

    # D33/D47: the oracle streams its human lines and its final sentence under
    # --json; this build suppresses the stream so stdout is one document.
    expect_statuses='0 0'
    envelope_rule='D33: the oracle prints the human stream under --json; this build keeps stdout one message envelope'
    expect_oracle_requests='length == 2 and (.[0].path == "/ado-harness/Watch/_apis/build/builds/201") and (.[1].path == "/ado-harness/Watch/_apis/build/builds/201/timeline")'
    expect_rust_requests='length == 2 and (.[0].path == "/ado-harness/Watch/_apis/build/builds/201") and (.[1].path == "/ado-harness/Watch/_apis/build/builds/201/timeline")'
    stdout_mode=text
    mock_case ci-watch-json "ci watch --json" \
        ci watch Watch 201 --json

    envelope_rule='D5: the oracle prints the command help on stdout before its usage error; this build writes clap’s message to stderr alone'
    expect_statuses='1 1'
    expect_oracle_requests='length == 0'
    expect_rust_requests='length == 0'
    stdout_mode=text
    mock_case ci-no-subcommand "ci (no sub-command)" \
        ci --json

    envelope_rule='D5: the oracle prints the command help on stdout before its usage error; this build writes clap’s message to stderr alone'
    expect_statuses='1 1'
    expect_oracle_requests='length == 0'
    expect_rust_requests='length == 0'
    stdout_mode=text
    mock_case ci-watch-no-project "ci watch (no project)" \
        ci watch --json

    # ── Task 9: the embedded skills surface ─────────────────────────────
    #
    # `skills` is the wave's only area that makes no HTTP request at all, so every
    # case asserts the direction the other way: both sides' request slices must be
    # empty. The five bespoke envelopes (and the sixth, `list <path>`) are compared
    # by the `json` mode; the human forms by `text`; the install cases write into a
    # target shared by both sides (`../install-*` resolves to the same $work/run
    # path for either cwd) with `--force`, so their documents match byte for byte
    # instead of naming the harness's per-side HOME. The `--json` error envelope
    # the frozen emits for an unresolved target is the area's one MATCH among the
    # refusals; the rest are D4 (the frozen's `xx  …` on stdout, no envelope) and
    # D5 (its help screen before the usage line).

    expect_statuses='0 0'
    expect_oracle_requests='length == 0'
    expect_rust_requests='length == 0'
    mock_case skills-list-json "skills list --json" \
        skills list --json

    expect_statuses='0 0'
    expect_oracle_requests='length == 0'
    expect_rust_requests='length == 0'
    stdout_mode=text
    mock_case skills-list "skills list" \
        skills list

    expect_statuses='0 0'
    expect_oracle_requests='length == 0'
    expect_rust_requests='length == 0'
    stdout_mode=text
    mock_case skills-list-skill "skills list <skill> (the ls-style entries)" \
        skills list ado-cli

    expect_statuses='0 0'
    expect_oracle_requests='length == 0'
    expect_rust_requests='length == 0'
    mock_case skills-list-skill-json "skills list <skill> --json" \
        skills list ado-cli --json

    expect_statuses='0 0'
    expect_oracle_requests='length == 0'
    expect_rust_requests='length == 0'
    mock_case skills-list-references-json "skills list <skill>/references --json" \
        skills list ado-cli/references --json

    envelope_rule='D4: the frozen prints its refusal on stdout and never an envelope, even under --json; this build writes the classified envelope'
    expect_statuses='1 1'
    expect_oracle_requests='length == 0'
    expect_rust_requests='length == 0'
    mock_case skills-list-unknown-json "skills list (unknown skill) --json" \
        skills list no-such --json

    envelope_rule='D4: the frozen prints its refusal on stdout; this build writes the labelled line to stderr'
    expect_statuses='1 1'
    expect_oracle_requests='length == 0'
    expect_rust_requests='length == 0'
    stdout_mode=text
    mock_case skills-list-unknown "skills list (unknown skill)" \
        skills list no-such

    envelope_rule='D5: the oracle prints the command help on stdout before its usage error; this build writes clap’s message to stderr alone'
    expect_statuses='1 1'
    expect_oracle_requests='length == 0'
    expect_rust_requests='length == 0'
    stdout_mode=text
    mock_case skills-list-extra "skills list (an extra positional)" \
        skills list ado-cli extra

    expect_statuses='0 0'
    expect_oracle_requests='length == 0'
    expect_rust_requests='length == 0'
    stdout_mode=text
    mock_case skills-describe "skills describe" \
        skills describe ado-cli

    expect_statuses='0 0'
    expect_oracle_requests='length == 0'
    expect_rust_requests='length == 0'
    mock_case skills-describe-json "skills describe --json" \
        skills describe ado-cli --json

    envelope_rule='D4: the frozen prints its refusal on stdout and never an envelope, even under --json; this build writes the classified envelope'
    expect_statuses='1 1'
    expect_oracle_requests='length == 0'
    expect_rust_requests='length == 0'
    mock_case skills-describe-unknown-json "skills describe (unknown skill) --json" \
        skills describe no-such --json

    envelope_rule='D5: the oracle prints the command help on stdout before its usage error; this build writes clap’s message to stderr alone'
    expect_statuses='1 1'
    expect_oracle_requests='length == 0'
    expect_rust_requests='length == 0'
    stdout_mode=text
    mock_case skills-describe-missing "skills describe (no name)" \
        skills describe

    expect_statuses='0 0'
    expect_oracle_requests='length == 0'
    expect_rust_requests='length == 0'
    stdout_mode=text
    mock_case skills-read "skills read (the stripped body)" \
        skills read ado-cli

    expect_statuses='0 0'
    expect_oracle_requests='length == 0'
    expect_rust_requests='length == 0'
    mock_case skills-read-json "skills read --json" \
        skills read ado-cli --json

    expect_statuses='0 0'
    expect_oracle_requests='length == 0'
    expect_rust_requests='length == 0'
    mock_case skills-read-reference-json "skills read <skill>/<file> --json (verbatim)" \
        skills read ado-cli/references/prs.md --json

    envelope_rule='D4: the frozen prints its file-not-found sentence on stdout; this build writes the labelled line to stderr'
    expect_statuses='1 1'
    expect_oracle_requests='length == 0'
    expect_rust_requests='length == 0'
    stdout_mode=text
    mock_case skills-read-missing-file "skills read (a missing file)" \
        skills read ado-cli/nope.md

    envelope_rule='D5: the oracle prints the command help on stdout before its usage error; this build writes clap’s message to stderr alone'
    expect_statuses='1 1'
    expect_oracle_requests='length == 0'
    expect_rust_requests='length == 0'
    stdout_mode=text
    mock_case skills-read-missing "skills read (no target)" \
        skills read

    expect_statuses='0 0'
    expect_oracle_requests='length == 0'
    expect_rust_requests='length == 0'
    mock_case skills-search-json "skills search --json" \
        skills search "create PR" --json

    expect_statuses='0 0'
    expect_oracle_requests='length == 0'
    expect_rust_requests='length == 0'
    mock_case skills-search-ci-json "skills search (priority across skills) --json" \
        skills search ci --json

    expect_statuses='0 0'
    expect_oracle_requests='length == 0'
    expect_rust_requests='length == 0'
    stdout_mode=text
    mock_case skills-search-ci "skills search (the group order is name-sorted)" \
        skills search ci

    expect_statuses='0 0'
    expect_oracle_requests='length == 0'
    expect_rust_requests='length == 0'
    stdout_mode=text
    mock_case skills-search-empty "skills search (no matches)" \
        skills search zzz

    envelope_rule='D5: the oracle prints the command help on stdout before its usage error; this build writes clap’s message to stderr alone'
    expect_statuses='1 1'
    expect_oracle_requests='length == 0'
    expect_rust_requests='length == 0'
    stdout_mode=text
    mock_case skills-search-missing "skills search (no query)" \
        skills search

    # The shared-relative-target trick: `../install-force` resolves to the same
    # $work/run/install-force for either side's cwd, and `--force` makes the second
    # side (the candidate) overwrite rather than report a skip, so the two
    # documents are the same bytes.
    expect_statuses='0 0'
    expect_oracle_requests='length == 0'
    expect_rust_requests='length == 0'
    mock_case skills-install-json "skills install --json (the install document)" \
        skills install --target ../install-force --force --json

    expect_statuses='0 0'
    expect_oracle_requests='length == 0'
    expect_rust_requests='length == 0'
    stdout_mode=text
    mock_case skills-install-human "skills install (the human summary)" \
        skills install --target ../install-human --force

    expect_statuses='0 0'
    expect_oracle_requests='length == 0'
    expect_rust_requests='length == 0'
    mock_case skills-install-copilot-json "skills install --target copilot --repo .. --json" \
        skills install --target copilot --repo .. --force --json

    expect_statuses='0 0'
    expect_oracle_requests='length == 0'
    expect_rust_requests='length == 0'
    mock_case skills-install-unknown-skill-json "skills install --skill (unknown) --json" \
        skills install --target ../install-unknown --skill no-such --json

    # The frozen's own validation_error document: both sides write the same
    # envelope, and the repo path in it is the one the case passed (identical for
    # both sides because it is relative to the shared parent).
    expect_statuses='1 1'
    expect_oracle_requests='length == 0'
    expect_rust_requests='length == 0'
    mock_case skills-install-copilot-missing-repo-json "skills install --repo (missing) --json" \
        skills install --target copilot --repo ../nope --json

    envelope_rule='D4: the frozen prints its target-resolution sentence on stdout; this build writes the labelled line to stderr'
    expect_statuses='1 1'
    expect_oracle_requests='length == 0'
    expect_rust_requests='length == 0'
    stdout_mode=text
    mock_case skills-install-copilot-missing-repo "skills install --repo (missing)" \
        skills install --target copilot --repo ../nope

    # The per-user target resolves under each side's isolated HOME, so the two
    # documents differ only in that path — the harness's isolation, not a contract
    # difference. The tree itself is asserted by the integration suite.
    envelope_rule='the pi target path names the harness’s per-side HOME; the layout is asserted by the integration suite'
    expect_statuses='0 0'
    expect_oracle_requests='length == 0'
    expect_rust_requests='length == 0'
    mock_case skills-install-per-user-json "skills install --target pi --json" \
        skills install --target pi --json

    envelope_rule='D5: the oracle prints the command help on stdout before its invalid-option line; this build writes clap’s message to stderr alone'
    expect_statuses='1 1'
    expect_oracle_requests='length == 0'
    expect_rust_requests='length == 0'
    stdout_mode=text
    mock_case skills-install-missing-value "skills install (a valueless --target)" \
        skills install --target

    # D43's family: the frozen reads `--json=false` and prints its human form;
    # this build's clap flag has one spelling, so the same invocation is refused.
    status_rule='D43: the frozen accepts the `--flag=false` boolean spelling; this build accepts only the bare flag'
    envelope_rule='D43: the oracle prints human output for --json=false; this build refuses the spelling (D5’s usage error)'
    expect_statuses='0 1'
    expect_oracle_requests='length == 0'
    expect_rust_requests='length == 0'
    stdout_mode=text
    mock_case skills-list-json-eq-false "skills list --json=false (D43)" \
        skills list --json=false

    envelope_rule='D5: the oracle prints the command help on stdout before its usage error; this build writes clap’s message to stderr alone'
    expect_statuses='1 1'
    expect_oracle_requests='length == 0'
    expect_rust_requests='length == 0'
    stdout_mode=text
    mock_case skills-no-subcommand "skills (no sub-command)" \
        skills --json

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
    #     underscore, `pipelines folders` gains the hyphen, and `repos policies`
    #     becomes `branch-policies` — at the root and in every descendant's name;
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
                   | .name |= gsub(\" repos policies\"; \" branch-policies\")
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
    d18_renamed=$(jq -r '[.schema | recurse(.subcommands[]?) | .name | select(test(" (pipelines (builds|artifacts|secure-files|folders)|repos policies)"))] | length' "$work/schema-json.elixir")

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
    # D14: the oracle's tree lists `ado security` twice, so the projection below
    # is deduped by name — clap cannot carry the duplicate and this build's tree
    # has one node. The D14 check above proves the oracle still duplicates, so the
    # dedupe cannot go stale, and the name check above still fails on any Rust
    # node the oracle does not have.
    node_projection='[ .schema.subcommands[]
        | select(.name as $node | $nodes | index($node))
        | . as $self
        | { name, arguments,
            options: ([ .options[] | select(.name as $o | $names | index($o) | not) ] | sort_by(.name)),
            subcommands: ([ $self.subcommands[].name ]
                | map(select(. as $child | ($children[$self.name] // []) | index($child))) | sort) } ]
      | sort_by(.name) | unique_by(.name)'
    node_projection_normalised='[ .schema.subcommands[]
        | select(.name as $node | $nodes | index($node))
        | . as $self
        | { name, arguments,
            options: ([ .options[]
                | select(.name as $o | $names | index($o) | not)
                | if .name == "write-to-file" then .name = "write_to_file" else . end ] | sort_by(.name)),
            subcommands: ([ $self.subcommands[].name ]
                | map(select(. as $child | ($children[$self.name] // []) | index($child))) | sort) } ]
      | sort_by(.name) | unique_by(.name)'

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
    # or the Rust doc is the oracle's prefix (D10). The reads take the first entry
    # per name, because the oracle lists `ado security` twice (D14) and a
    # concatenated double doc would read as a truncation that is not there.
    while IFS= read -r node; do
        el_doc=$(jq -r --arg node "$node" 'first(.schema.subcommands[] | select(.name == $node) | .doc)' "$schema_el")
        rs_doc=$(jq -r --arg node "$node" 'first(.schema.subcommands[] | select(.name == $node) | .doc)' "$schema_rs")

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
