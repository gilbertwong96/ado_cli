/// Flags whose value runs until the next flag; joined into one `--flag=value` token.
pub const MULTIVALUE_FLAGS: [&str; 7] = [
    "--content",
    "--message",
    "--body",
    "--description",
    "--reason",
    "--summary",
    "--text",
];

pub fn normalize(args: Vec<String>) -> Vec<String> {
    let mut normalized = Vec::with_capacity(args.len());
    let mut remaining = args.as_slice();

    while let Some((arg, rest)) = remaining.split_first() {
        match multivalue_flag(arg) {
            Some(flag) => {
                let (values, leftover) = take_values(rest);
                normalized.push(format!("{flag}={}", values.join(" ")));
                remaining = leftover;
            }
            None => {
                normalized.push(arg.clone());
                remaining = rest;
            }
        }
    }

    normalized
}

pub fn is_version_flag(args: &[String]) -> bool {
    args.iter().any(|arg| arg == "--version")
}

fn multivalue_flag(arg: &str) -> Option<&'static str> {
    MULTIVALUE_FLAGS.iter().copied().find(|flag| *flag == arg)
}

fn take_values(tokens: &[String]) -> (&[String], &[String]) {
    let end = tokens
        .iter()
        .position(|token| is_flag(token))
        .unwrap_or(tokens.len());

    tokens.split_at(end)
}

fn is_flag(token: &str) -> bool {
    token.starts_with('-') && token != "-"
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(raw: &[&str]) -> Vec<String> {
        raw.iter().map(|token| String::from(*token)).collect()
    }

    #[test]
    fn joins_multivalue_tokens() {
        assert_eq!(
            normalize(args(&["--description", "fix", "the", "thing"])),
            args(&["--description=fix the thing"])
        );
    }

    #[test]
    fn stops_at_the_next_flag() {
        assert_eq!(
            normalize(args(&["--description", "a", "b", "--status", "done"])),
            args(&["--description=a b", "--status", "done"])
        );
    }

    #[test]
    fn bare_dash_is_a_value() {
        assert_eq!(
            normalize(args(&["--message", "-", "x"])),
            args(&["--message=- x"])
        );
    }

    #[test]
    fn non_multivalue_flags_untouched() {
        assert_eq!(
            normalize(args(&["--org", "myorg"])),
            args(&["--org", "myorg"])
        );
    }

    #[test]
    fn empty_multivalue_becomes_empty() {
        assert_eq!(normalize(args(&["--summary"])), args(&["--summary="]));
    }
}
