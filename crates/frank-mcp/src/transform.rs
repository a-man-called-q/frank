//! JSON-RPC response rewriting: compress every configured field (default
//! `description`) inside a `tools/list`-shaped response. Ported from
//! historical Caveman MCP proxy's `transformResponse`
//! and `compress.js`'s `compressDescriptionsInPlace`.
//!
//! This is not a real MCP client — it never negotiates capabilities or
//! dispatches by method. It only ever rewrites `result.{tools,prompts,
//! resources,resourceTemplates}[]` in a passing response and, when nothing
//! matches there, falls back to a recursive walk. Everything else passes
//! through untouched.

use serde_json::Value;

const KNOWN_ARRAYS: &[&str] = &["tools", "prompts", "resources", "resourceTemplates"];

pub fn transform_response(msg: &mut Value, fields: &[String]) {
    let Some(result) = msg.get_mut("result") else {
        return;
    };
    if !result.is_object() {
        return;
    }

    let mut matched = false;
    for key in KNOWN_ARRAYS {
        if let Some(arr) = result.get_mut(*key).and_then(Value::as_array_mut) {
            for item in arr.iter_mut() {
                if compress_fields_in_object(item, fields) {
                    matched = true;
                }
            }
        }
    }
    if !matched {
        compress_descriptions_in_place(result, fields);
    }
}

fn compress_fields_in_object(item: &mut Value, fields: &[String]) -> bool {
    let Some(obj) = item.as_object_mut() else {
        return false;
    };
    let mut found = false;
    for field in fields {
        if let Some(Value::String(s)) = obj.get(field) {
            found = true;
            let compressed = frank_compress::compress(s).compressed;
            obj.insert(field.clone(), Value::String(compressed));
        }
    }
    found
}

/// Recursive fallback: walk every object/array, compressing any field
/// whose key matches `fields`. Used only when the top-level `tools`-shaped
/// walk found nothing, avoiding double-compression of nested parameter
/// schemas that also happen to contain a `description` key.
pub fn compress_descriptions_in_place(value: &mut Value, fields: &[String]) {
    match value {
        Value::Array(arr) => {
            for item in arr.iter_mut() {
                compress_descriptions_in_place(item, fields);
            }
        }
        Value::Object(obj) => {
            let keys: Vec<String> = obj.keys().cloned().collect();
            for key in keys {
                if fields.contains(&key) {
                    if let Some(Value::String(s)) = obj.get(&key) {
                        let compressed = frank_compress::compress(s).compressed;
                        obj.insert(key.clone(), Value::String(compressed));
                        continue;
                    }
                }
                if let Some(v) = obj.get_mut(&key) {
                    if v.is_object() || v.is_array() {
                        compress_descriptions_in_place(v, fields);
                    }
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fields() -> Vec<String> {
        vec!["description".to_string()]
    }

    #[test]
    fn compresses_description_in_tools_list() {
        let mut msg = json!({
            "result": {
                "tools": [
                    { "name": "fetch", "description": "Please just fetch the URL, it will basically retrieve the content." }
                ]
            }
        });
        transform_response(&mut msg, &fields());
        let desc = msg["result"]["tools"][0]["description"].as_str().unwrap();
        assert!(!desc.to_lowercase().contains("please"));
        assert!(!desc.to_lowercase().contains("basically"));
        assert!(desc.to_lowercase().contains("url"));
    }

    #[test]
    fn falls_back_to_recursive_walk_when_no_known_array_present() {
        let mut msg = json!({
            "result": {
                "nested": { "thing": { "description": "Sure, this is just a nested description." } }
            }
        });
        transform_response(&mut msg, &fields());
        let desc = msg["result"]["nested"]["thing"]["description"]
            .as_str()
            .unwrap();
        assert!(!desc.to_lowercase().contains("sure"));
    }

    #[test]
    fn recursive_fallback_does_not_double_compress_when_top_level_matched() {
        // A tool with a nested parameter schema that ALSO has a
        // `description` key — only the top-level tool description should
        // be touched by the top-level walk; the fallback must not run
        // (and re-compress) since something already matched.
        let mut msg = json!({
            "result": {
                "tools": [
                    {
                        "name": "fetch",
                        "description": "Please fetch the URL.",
                        "inputSchema": { "properties": { "url": { "description": "Please provide the URL." } } }
                    }
                ]
            }
        });
        transform_response(&mut msg, &fields());
        // Top-level compressed:
        assert!(
            !msg["result"]["tools"][0]["description"]
                .as_str()
                .unwrap()
                .to_lowercase()
                .contains("please")
        );
        // Nested schema description untouched, since the top-level match
        // short-circuits the recursive fallback:
        assert_eq!(
            msg["result"]["tools"][0]["inputSchema"]["properties"]["url"]["description"],
            "Please provide the URL."
        );
    }

    #[test]
    fn non_string_description_fields_are_left_alone() {
        let mut msg = json!({ "result": { "tools": [ { "description": 42 } ] } });
        transform_response(&mut msg, &fields());
        assert_eq!(msg["result"]["tools"][0]["description"], 42);
    }

    #[test]
    fn missing_result_is_a_noop() {
        let mut msg = json!({ "id": 1, "method": "ping" });
        let before = msg.clone();
        transform_response(&mut msg, &fields());
        assert_eq!(msg, before);
    }

    #[test]
    fn custom_field_list_is_respected() {
        let mut msg =
            json!({ "result": { "tools": [ { "summary": "Please summarize just this." } ] } });
        transform_response(&mut msg, &["summary".to_string()]);
        assert!(
            !msg["result"]["tools"][0]["summary"]
                .as_str()
                .unwrap()
                .to_lowercase()
                .contains("please")
        );
    }
}
