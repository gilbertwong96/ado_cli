//! The list unwrapper the read areas share.
//!
//! `workitems` keeps its own inline variant on purpose: its fallback semantics
//! differ (an empty WIQL result is handled before the batch fetch), so it is not
//! a caller of this helper.

use serde_json::Value;

/// The second half of the Elixir's `Client.list/2`: the `value` array has already
/// been unwrapped by `ado_core::client::Client::list`, so an array is the list,
/// and anything else — including a `null` body, which `List.wrap/1` would drop —
/// is wrapped as its single element, so the value envelope always carries an
/// array.
pub fn items(value: Value) -> Vec<Value> {
    match value {
        Value::Array(items) => items,
        other => vec![other],
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::items;

    #[test]
    fn an_array_body_passes_through_unchanged() {
        assert_eq!(
            items(json!([{"id": 1}, {"id": 2}])),
            vec![json!({"id": 1}), json!({"id": 2})],
            "the value array is the list"
        );
        assert_eq!(
            items(json!([])),
            Vec::<Value>::new(),
            "an empty list stays empty"
        );
    }

    #[test]
    fn a_non_array_body_becomes_its_single_element() {
        assert_eq!(
            items(Value::Null),
            vec![Value::Null],
            "a null body is one element, unlike List.wrap/1 which drops it"
        );
        assert_eq!(
            items(json!({"count": 1, "value": [1]})),
            vec![json!({"count": 1, "value": [1]})],
            "a body that is not an array is wrapped whole, not re-unwrapped"
        );
        assert_eq!(items(json!("text")), vec![json!("text")]);
    }
}
