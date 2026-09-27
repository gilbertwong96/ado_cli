use clap::ArgMatches;

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
