#!/usr/bin/env bash
# Wave 0 oracle diff — the frozen Elixir `ado` (the oracle) against the Rust
# release binary, for every command Wave 0 ported: `version`, `whoami`, `schema`
# and `completion`.
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
# 2 when the script cannot run (missing binary or jq).
#
# Usage: scripts/oracle-diff.sh
#   ADO_ORACLE_ELIXIR=<path>   override the oracle   (default ./ado)
#   ADO_ORACLE_RUST=<path>     override the candidate (default target/release/ado)

set -uo pipefail

root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
elixir_bin=${ADO_ORACLE_ELIXIR:-$root/ado}
rust_bin=${ADO_ORACLE_RUST:-$root/target/release/ado}

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
trap 'rm -rf "$work"' EXIT

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

    # Every Rust node must be a node the oracle has (the tree only shrinks).
    jq -r '.schema.subcommands[].name' "$schema_el" | sort -u >"$work/schema.names.elixir"
    jq -r '.schema.subcommands[].name' "$schema_rs" | sort -u >"$work/schema.names.rust"
    unlisted=$(comm -13 "$work/schema.names.elixir" "$work/schema.names.rust" | tr '\n' ' ' | sed 's/ *$//')

    el_count=$(jq '.schema.subcommands | length' "$schema_el")
    rs_count=$(jq '.schema.subcommands | length' "$schema_rs")

    if [[ $rs_count == 0 ]]; then
        fail "the Rust tree has no subcommands"
    elif [[ -n $unlisted ]]; then
        fail "the Rust tree names nodes the oracle does not: $unlisted"
    else
        note "every Rust node is an oracle node ($rs_count of $el_count)"
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
    globals_names=$(jq -c '[.schema.options[].name]' "$schema_rs")
    globals_entries=$(jq -cS '[.schema.options[]] | sort_by(.name)' "$schema_rs")

    while IFS= read -r node; do
        node_globals=$(jq -cS --arg node "$node" --argjson names "$globals_names" \
            '[.schema.subcommands[] | select(.name == $node) | .options[] | select(.name as $o | $names | index($o))] | sort_by(.name)' "$schema_rs")

        if [[ $node_globals != "$globals_entries" ]]; then
            fail "node '$node' carries different globals than the root (D2): $node_globals"
        fi
    done < <(jq -r '.schema.subcommands[].name' "$schema_rs")

    # The nodes both sides have: name, arguments, children and the options that
    # are not globals. Docs are compared separately, because D10 lets the Rust
    # doc be the oracle's truncated at the end of the usage block. The only ruled
    # value difference left is the spelling of `write-to-file` (D17), so the raw
    # comparison is expected to fail there and the hyphen/underscore-normalised
    # one is expected to pass.
    shared_names=$(jq -c '[.schema.subcommands[].name]' "$schema_rs")
    node_projection='[ .schema.subcommands[]
        | select(.name as $node | $nodes | index($node))
        | { name, arguments,
            options: ([ .options[] | select(.name as $o | $names | index($o) | not) ] | sort_by(.name)),
            subcommands: [ .subcommands[].name ] } ]
      | sort_by(.name)'
    node_projection_normalised='[ .schema.subcommands[]
        | select(.name as $node | $nodes | index($node))
        | { name, arguments,
            options: ([ .options[]
                | select(.name as $o | $names | index($o) | not)
                | if .name == "write-to-file" then .name = "write_to_file" else . end ] | sort_by(.name)),
            subcommands: [ .subcommands[].name ] } ]
      | sort_by(.name)'

    jq -S --argjson nodes "$shared_names" --argjson names "$globals_names" "$node_projection" "$schema_el" >"$work/schema.nodes.elixir"
    jq -S --argjson nodes "$shared_names" --argjson names "$globals_names" "$node_projection" "$schema_rs" >"$work/schema.nodes.rust"
    jq -S --argjson nodes "$shared_names" --argjson names "$globals_names" "$node_projection_normalised" "$schema_el" >"$work/schema.nodes.norm.elixir"
    jq -S --argjson nodes "$shared_names" --argjson names "$globals_names" "$node_projection_normalised" "$schema_rs" >"$work/schema.nodes.norm.rust"

    if same "$work/schema.nodes.elixir" "$work/schema.nodes.rust"; then
        note "the shared nodes match exactly"
    elif same "$work/schema.nodes.norm.elixir" "$work/schema.nodes.norm.rust"; then
        ruled "option name spelling (D17): 'write_to_file' vs 'write-to-file'"
    else
        fail "the shared nodes differ beyond the ruled spelling: $(first_difference "$work/schema.nodes.norm.elixir" "$work/schema.nodes.norm.rust")"
    fi

    # Node docs: equal, or the Rust doc is the oracle's prefix (D10).
    while IFS= read -r node; do
        el_doc=$(jq -r --arg node "$node" '.schema.subcommands[] | select(.name == $node) | .doc' "$schema_el")
        rs_doc=$(jq -r --arg node "$node" '.schema.subcommands[] | select(.name == $node) | .doc' "$schema_rs")

        if [[ $el_doc == "$rs_doc" ]]; then
            continue
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

printf '\noracle-diff: %d cases — %d match, %d expected-diff, %d unrecorded diff\n' \
    "$((matches + expected + differences))" "$matches" "$expected" "$differences"

if (( differences > 0 )); then
    printf 'oracle-diff: FAIL — %d unrecorded difference(s); update docs/rust-rewrite/contract-inventory.md or fix the drift\n' "$differences" >&2
    exit 1
fi

printf 'oracle-diff: OK — every difference is recorded in docs/rust-rewrite/contract-inventory.md §9/§10\n'
