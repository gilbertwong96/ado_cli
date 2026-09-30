//! `AdoCli.Fuzzy`'s `match_fields/3` — the client-side filter the frozen
//! `prs reviewers list --search` runs over `displayName` and `uniqueName`.
//!
//! Only the boolean half of the module is ported: `match_fields/3` keeps an item
//! whose any field scores above zero, and for a non-empty query a positive score
//! is exactly "the candidate equals, starts with or contains the query, or the
//! query is a subsequence of it". The module's ranking (`Fuzzy.match/2`) and its
//! `@spec`ed return of `{candidate, score}` pairs have **no caller anywhere in
//! `lib/`** — dead code in the oracle, not a later wave's work — so they are not
//! ported. An absent or empty query is no filter at all, as captured.

use serde_json::Value;

/// Keeps the items whose `fields` match `query` — case-insensitive substring or
/// subsequence — preserving the original order. A `None` or empty query returns
/// every item; a field that is absent or not a string cannot match.
pub fn match_fields(items: &[Value], query: Option<&str>, fields: &[&str]) -> Vec<Value> {
    let query = match query {
        None | Some("") => return items.to_vec(),
        Some(query) => query.to_lowercase(),
    };

    items
        .iter()
        .filter(|item| {
            fields
                .iter()
                .any(|field| field_matches(item, field, &query))
        })
        .cloned()
        .collect()
}

fn field_matches(item: &Value, field: &str, query: &str) -> bool {
    item.get(field)
        .and_then(Value::as_str)
        .is_some_and(|value| matches(value, query))
}

/// The module's `score/2 > 0` for a non-empty query: equality, a prefix and a
/// substring are all `contains`, and the rest is the subsequence case.
fn matches(candidate: &str, query: &str) -> bool {
    let candidate = candidate.to_lowercase();

    candidate.contains(query) || is_subsequence(&candidate, query)
}

fn is_subsequence(candidate: &str, query: &str) -> bool {
    let mut candidate = candidate.chars();

    query
        .chars()
        .all(|query_char| candidate.any(|candidate_char| candidate_char == query_char))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::match_fields;

    const FIELDS: [&str; 2] = ["displayName", "uniqueName"];

    fn reviewers() -> Vec<serde_json::Value> {
        vec![
            json!({"displayName": "Ada Example", "uniqueName": "ada@example.com"}),
            json!({"displayName": "Bob Jones", "uniqueName": "bob@example.com"}),
            json!({"displayName": "Carol Ng", "uniqueName": "carol@example.com"}),
        ]
    }

    #[test]
    fn an_absent_or_empty_query_is_no_filter() {
        for query in [None, Some("")] {
            assert_eq!(
                match_fields(&reviewers(), query, &FIELDS),
                reviewers(),
                "query: {query:?}"
            );
        }
    }

    #[test]
    fn a_substring_matches_case_insensitively() {
        assert_eq!(
            match_fields(&reviewers(), Some("ADA"), &FIELDS),
            vec![reviewers()[0].clone()]
        );
        assert_eq!(
            match_fields(&reviewers(), Some("jones"), &FIELDS),
            vec![reviewers()[1].clone()]
        );
    }

    #[test]
    fn a_query_that_is_a_subsequence_matches() {
        assert_eq!(
            match_fields(&reviewers(), Some("aae"), &FIELDS),
            vec![reviewers()[0].clone(), reviewers()[2].clone()],
            "captured: aae is a subsequence of `Ada Example` and of `carol@example.com`"
        );
    }

    #[test]
    fn every_named_field_is_consulted() {
        assert_eq!(
            match_fields(&reviewers(), Some("carol@"), &FIELDS),
            vec![reviewers()[2].clone()],
            "the uniqueName is the only field that matches"
        );
    }

    #[test]
    fn a_non_string_or_absent_field_cannot_match() {
        let items = vec![
            json!({"displayName": 7, "uniqueName": "seven@example.com"}),
            json!({"displayName": "Seven"}),
        ];

        assert_eq!(
            match_fields(&items, Some("7"), &FIELDS),
            Vec::<serde_json::Value>::new(),
            "the number 7 is not a string field"
        );
        assert_eq!(
            match_fields(&items, Some("seven"), &FIELDS),
            items,
            "an absent field is skipped, not an error"
        );
    }

    #[test]
    fn no_match_is_an_empty_list() {
        assert_eq!(
            match_fields(&reviewers(), Some("zzz"), &FIELDS),
            Vec::<serde_json::Value>::new()
        );
    }

    #[test]
    fn the_original_order_is_preserved() {
        let matches = match_fields(&reviewers(), Some("example.com"), &FIELDS);

        assert_eq!(
            matches,
            reviewers(),
            "match_fields/3 filters; it does not rank (Fuzzy.match/2's job)"
        );
    }
}
