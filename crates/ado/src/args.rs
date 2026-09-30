use clap::ArgMatches;
use clap::parser::ValueSource;

#[derive(Debug, PartialEq)]
pub struct GlobalOpts {
    pub org: Option<String>,
    pub pat: Option<String>,
    pub server: Option<String>,
    pub verbose: bool,
    pub json: bool,
}

impl GlobalOpts {
    pub fn from_matches(matches: &ArgMatches) -> Self {
        Self {
            org: matches.get_one::<String>("org").cloned(),
            pat: matches.get_one::<String>("pat").cloned(),
            server: matches.get_one::<String>("server").cloned(),
            verbose: matches.get_flag("verbose"),
            json: matches.get_flag("json"),
        }
    }
}

/// A `--flag`/`--no-flag` pair as one tri-state: `Some(true)` for the positive
/// spelling, `Some(false)` for the negative, `None` when neither was given. The
/// pair is declared with mutual `overrides_with`, so when both appear only the
/// last one carries `ValueSource::CommandLine` — the POSIX last-wins rule, the
/// same answer the oracle's `Map.new/1` over OptionParser's list produces.
pub fn negatable_flag(matches: &ArgMatches, positive: &str, negative: &str) -> Option<bool> {
    if matches.value_source(negative) == Some(ValueSource::CommandLine) {
        Some(false)
    } else if matches.value_source(positive) == Some(ValueSource::CommandLine) {
        Some(true)
    } else {
        None
    }
}
