use serde_json::{json, Value as JValue};
use crate::eval::{Value, VariantPayload};

// =============================================================================
// Value → serde_json::Value
// =============================================================================

pub fn keln_value_to_json(v: &Value) -> JValue {
    match v {
        Value::Unit => JValue::Null,
        Value::Bool(b) => json!(b),
        Value::Int(n) => json!(n),
        Value::Float(f) => json!(f),
        Value::Str(s) => json!(s),
        Value::Bytes(b) => {
            // Encode as array of numbers for simplicity
            let nums: Vec<JValue> = b.iter().map(|x| json!(x)).collect();
            JValue::Array(nums)
        }
        Value::List(items) => {
            JValue::Array(items.iter().map(keln_value_to_json).collect())
        }
        Value::Record(fields) => {
            let mut map = serde_json::Map::new();
            for (k, v) in fields {
                map.insert(k.clone(), keln_value_to_json(v));
            }
            JValue::Object(map)
        }
        Value::Variant { name, payload } => match payload {
            VariantPayload::Unit => {
                json!({ "$tag": name })
            }
            VariantPayload::Tuple(inner) => {
                json!({ "$tag": name, "$value": keln_value_to_json(inner) })
            }
            VariantPayload::Record(fields) => {
                let mut map = serde_json::Map::new();
                map.insert("$tag".to_string(), json!(name));
                for (k, v) in fields {
                    map.insert(k.clone(), keln_value_to_json(v));
                }
                JValue::Object(map)
            }
        },
        Value::FnRef(n) => json!({ "$fnref": n }),
        Value::TypeRef(n) => json!({ "$typeref": n }),
        Value::Duration(ms) => json!({ "$duration_ms": ms }),
        Value::Timestamp(ms) => json!({ "$timestamp_ms": ms }),
        Value::Channel(_) => json!({ "$channel": true }),
        Value::Task(inner) => json!({ "$task": keln_value_to_json(inner) }),
        Value::Map(pairs) => {
            let entries: Vec<JValue> = pairs
                .iter()
                .map(|(k, v)| json!([keln_value_to_json(k), keln_value_to_json(v)]))
                .collect();
            json!({ "$map": entries })
        }
        Value::Set(items) => {
            let arr: Vec<JValue> = items.iter().map(keln_value_to_json).collect();
            json!({ "$set": arr })
        }
        Value::PartialFn { name, .. } => json!({ "$partial": name }),
    }
}

// =============================================================================
// serde_json::Value → Value
// =============================================================================

pub fn json_to_keln_value(j: JValue) -> Value {
    match j {
        JValue::Null => Value::Unit,
        JValue::Bool(b) => Value::Bool(b),
        JValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Int(i)
            } else if let Some(f) = n.as_f64() {
                Value::Float(f)
            } else {
                Value::Unit
            }
        }
        JValue::String(s) => Value::Str(s),
        JValue::Array(arr) => {
            Value::List(arr.into_iter().map(json_to_keln_value).collect())
        }
        JValue::Object(map) => {
            if let Some(tag) = map.get("$tag") {
                let name = match tag {
                    JValue::String(s) => s.clone(),
                    other => other.to_string(),
                };

                // Check if there's a "$value" key (Tuple payload)
                if let Some(val) = map.get("$value") {
                    return Value::Variant {
                        name,
                        payload: VariantPayload::Tuple(Box::new(json_to_keln_value(val.clone()))),
                    };
                }

                // Collect remaining fields as Record payload (skip "$tag")
                let fields: Vec<(String, Value)> = map
                    .into_iter()
                    .filter(|(k, _)| k != "$tag")
                    .map(|(k, v)| (k, json_to_keln_value(v)))
                    .collect();

                if fields.is_empty() {
                    Value::Variant { name, payload: VariantPayload::Unit }
                } else {
                    Value::Variant { name, payload: VariantPayload::Record(fields) }
                }
            } else {
                // Plain object → Record
                let fields: Vec<(String, Value)> = map
                    .into_iter()
                    .map(|(k, v)| (k, json_to_keln_value(v)))
                    .collect();
                Value::Record(fields)
            }
        }
    }
}
