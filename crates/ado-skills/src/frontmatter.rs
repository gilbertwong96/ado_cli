//! The frozen `AdoCli.Frontmatter`: a deliberately small YAML subset — a fence
//! line, `key: value` pairs, one optional block list — parsed without a YAML
//! library. The quirks are the contract and the captures pin them:
//!
//!   * a `commands:` item containing a `:` is claimed by the key branch and never
//!     joins the list (`ado-auth`'s `export ADO_SERVER=https://…` is the shipped
//!     instance: 8 commands, not 9, and `search "export ADO_SERVER"` finds
//!     nothing);
//!   * the first occurrence of a key wins (`Map.put_new/3`);
//!   * the map is keyed by the trimmed key, **not** lowercased, although the
//!     module's own comment claims otherwise;
//!   * `strip_quotes/1` recurses and drops an unbalanced leading quote;
//!   * the content is frontmatter only when its first line is exactly `---`
//!     (a `---\r` line is not), and a fence with no closing line takes the rest
//!     of the file as frontmatter.

use std::collections::BTreeMap;

/// The fence delimiting the frontmatter, exactly as `String.split/2` compares it.
const FENCE: &str = "---";

/// `AdoCli.Frontmatter.parse/1`: the frontmatter's `key => value` map, or an
/// empty map when the content does not start with a fence line. Values are
/// trimmed and unquoted; the `commands` value is the raw newline-joined block.
pub fn parse(content: &str) -> BTreeMap<String, String> {
    let mut lines = content.split('\n');

    if lines.next() != Some(FENCE) {
        return BTreeMap::new();
    }

    parse_lines(&lines.take_while(|line| *line != FENCE).collect::<Vec<_>>())
}

/// `AdoCli.Frontmatter.parse_commands/1`: the `commands` block as clean command
/// strings, or an empty list when the field is absent or empty.
pub fn parse_commands(content: &str) -> Vec<String> {
    let map = parse(content);

    match map.get("commands") {
        Some(raw) if !raw.is_empty() => raw
            .split('\n')
            .filter(|line| !line.is_empty())
            .map(strip_command_prefix)
            .filter(|command| !command.is_empty())
            .collect(),
        _ => Vec::new(),
    }
}

/// The read path's strip (`AdoCli.CLI.Skills.strip_frontmatter/1`): drop the
/// fence and the blank lines after it, join and trim. Content without a fence is
/// returned unchanged; a fence with no closing line strips to the empty string
/// (the frozen's `Enum.split_while/2` returns an empty tail).
pub fn strip_frontmatter(content: &str) -> String {
    let mut lines = content.split('\n');

    if lines.next() != Some(FENCE) {
        return content.to_owned();
    }

    lines
        .skip_while(|line| *line != FENCE)
        .skip_while(|line| *line == FENCE || line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_owned()
}

fn parse_lines(lines: &[&str]) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();

    for line in lines {
        match line.split_once(':') {
            Some((key, value)) => {
                map.entry(key.trim().to_owned())
                    .or_insert_with(|| collect_value(value));
            }
            None => {
                if let Some(commands) = map.get_mut("commands") {
                    commands.push('\n');
                    commands.push_str(line);
                }
            }
        }
    }

    map
}

fn collect_value(value: &str) -> String {
    let stripped = value.trim();

    if stripped.is_empty() {
        String::new()
    } else {
        strip_quotes(stripped)
    }
}

fn strip_quotes(value: &str) -> String {
    if let Some(rest) = value.strip_prefix('"') {
        return match rest.strip_suffix('"') {
            Some(inner) => strip_quotes(inner),
            None => rest.to_owned(),
        };
    }

    if let Some(rest) = value.strip_prefix('\'') {
        return match rest.strip_suffix('\'') {
            Some(inner) => strip_quotes(inner),
            None => rest.to_owned(),
        };
    }

    value.to_owned()
}

fn strip_command_prefix(line: &str) -> String {
    if let Some(rest) = line.strip_prefix("  - ") {
        rest.trim().to_owned()
    } else if let Some(rest) = line.strip_prefix("- ") {
        rest.trim().to_owned()
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect()
    }

    #[test]
    fn parses_a_fence_block_and_stops_at_the_closing_fence() {
        assert_eq!(
            parse("---\nname: x\ndescription: \"a: b\"\n---\nbody\n---\nmore"),
            map(&[("name", "x"), ("description", "a: b")])
        );
    }

    #[test]
    fn a_missing_fence_is_an_empty_map() {
        assert_eq!(parse("# title\nkey: value\n"), map(&[]));
        assert_eq!(parse(""), map(&[]));
        assert_eq!(
            parse("---\r\nkey: value\r\n---\r\n"),
            map(&[]),
            "CRLF's first line is '---\\r', not the fence"
        );
    }

    #[test]
    fn a_fence_with_no_closing_line_takes_the_rest_of_the_file() {
        assert_eq!(
            parse("---\nkey: value\nmore: text\n"),
            map(&[("key", "value"), ("more", "text")])
        );
    }

    #[test]
    fn the_first_occurrence_of_a_key_wins() {
        assert_eq!(
            parse("---\nkey: first\nkey: second\n---\n"),
            map(&[("key", "first")])
        );
    }

    #[test]
    fn keys_are_trimmed_but_not_lowercased() {
        assert_eq!(
            parse("---\n  Description : value\n---\n"),
            map(&[("Description", "value")]),
            "the module comment claims a lowercase key; the code only trims"
        );
    }

    #[test]
    fn an_empty_value_is_an_empty_string() {
        assert_eq!(
            parse("---\nkey:\nkey2:   \n---\n"),
            map(&[("key", ""), ("key2", "")])
        );
    }

    #[test]
    fn quotes_are_stripped_recursively_and_unbalanced_ones_dropped() {
        assert_eq!(
            parse("---\na: \"x\"\nb: 'y'\nc: \"\"x\"\"\nd: \"unbalanced\ne: \"\n---\n"),
            map(&[
                ("a", "x"),
                ("b", "y"),
                ("c", "x"),
                ("d", "unbalanced"),
                ("e", ""),
            ])
        );
    }

    #[test]
    fn a_line_without_a_colon_joins_only_a_commands_block() {
        assert_eq!(
            parse("---\nkey: value\nstray line\n---\n"),
            map(&[("key", "value")]),
            "there is no commands key to append to"
        );
        assert_eq!(
            parse("---\ncommands:\n  - a\n  - b\nother: x\nstray\n---\n"),
            map(&[("commands", "\n  - a\n  - b\nstray"), ("other", "x")]),
            "a stray line after another key appends to commands, the frozen's own quirk"
        );
    }

    #[test]
    fn a_command_line_with_a_colon_is_never_a_command() {
        let content = "---\ncommands:\n  - ado login\n  - export ADO_SERVER=https://dev.azure.com\n  - ado logout\n---\nbody";

        assert_eq!(
            parse_commands(content),
            ["ado login", "ado logout"],
            "the colon line is claimed by the key branch (the ado-auth asset's shape)"
        );
    }

    #[test]
    fn parse_commands_strips_the_two_prefixes_and_rejects_other_lines() {
        let content = "---\ncommands:\n  - ado login\n- ado logout\nplain\n  -   spaced\n---\nbody";

        assert_eq!(
            parse_commands(content),
            ["ado login", "ado logout", "spaced"]
        );
    }

    #[test]
    fn parse_commands_without_a_field_or_with_an_empty_one_is_empty() {
        assert_eq!(parse_commands("no frontmatter"), Vec::<String>::new());
        assert_eq!(
            parse_commands("---\ncommands:\n---\nbody"),
            Vec::<String>::new()
        );
    }

    #[test]
    fn the_commands_value_is_newline_joined_from_an_empty_first_line() {
        assert_eq!(
            parse("---\ncommands:\n  - a\n---\n").get("commands"),
            Some(&"\n  - a".to_owned())
        );
    }

    #[test]
    fn strip_frontmatter_drops_the_fence_and_the_blank_lines_after_it() {
        assert_eq!(
            strip_frontmatter("---\nkey: value\n---\n\n# Title\n\nbody\n"),
            "# Title\n\nbody"
        );
    }

    #[test]
    fn strip_frontmatter_without_a_fence_is_the_content_unchanged() {
        assert_eq!(strip_frontmatter("# Title\n"), "# Title\n");
    }

    #[test]
    fn strip_frontmatter_with_no_closing_fence_is_empty() {
        assert_eq!(
            strip_frontmatter("---\nkey: value\n# Title\n"),
            "",
            "the frozen's split_while leaves an empty tail"
        );
        assert_eq!(strip_frontmatter("---\nkey: value"), "");
    }

    #[test]
    fn strip_frontmatter_of_a_body_less_file_is_empty() {
        assert_eq!(strip_frontmatter("---\nkey: value\n---\n"), "");
    }
}
