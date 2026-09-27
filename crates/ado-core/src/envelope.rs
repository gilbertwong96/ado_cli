use serde_json::{Map, Value, json};

pub fn ok_value(value: Value) -> Value {
    json!({ "ok": true, "result": value })
}

pub fn ok_list(items: Vec<Value>) -> Value {
    json!({ "ok": true, "count": items.len(), "items": items })
}

pub fn ok_message(message: &str) -> Value {
    json!({ "ok": true, "message": message })
}

pub fn ok_named(key: &str, value: Value) -> Value {
    let mut envelope = Map::new();
    envelope.insert("ok".to_owned(), Value::Bool(true));
    envelope.insert(key.to_owned(), value);
    Value::Object(envelope)
}

pub fn error_json(code: &str, status: Option<u16>, message: &str, details: Option<Value>) -> Value {
    let mut error = Map::new();
    error.insert("code".to_owned(), Value::String(code.to_owned()));
    error.insert("message".to_owned(), Value::String(message.to_owned()));

    if let Some(status) = status {
        error.insert("status".to_owned(), json!(status));
    }
    if let Some(details) = details {
        error.insert("details".to_owned(), details);
    }

    json!({ "ok": false, "error": Value::Object(error) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn ok_value_wraps_under_result() {
        assert_eq!(
            ok_value(json!({ "a": 1 })),
            json!({ "ok": true, "result": { "a": 1 } })
        );
    }

    #[test]
    fn ok_list_adds_count_and_items() {
        assert_eq!(
            ok_list(vec![json!({ "id": 1 }), json!({ "id": 2 })]),
            json!({ "ok": true, "count": 2, "items": [{ "id": 1 }, { "id": 2 }] })
        );
    }

    #[test]
    fn ok_named_uses_the_given_key() {
        assert_eq!(
            ok_named("version", json!("1.0.0-rc.0")),
            json!({ "ok": true, "version": "1.0.0-rc.0" })
        );
    }

    #[test]
    fn error_omits_absent_status_and_details() {
        let value = error_json("not_found", None, "Project 'x' not found.", None);
        let error = value["error"].as_object().expect("error object");

        assert_eq!(value["ok"], json!(false));
        assert_eq!(error["code"], json!("not_found"));
        assert_eq!(error["message"], json!("Project 'x' not found."));
        assert!(!error.contains_key("status"));
        assert!(!error.contains_key("details"));
    }
}
