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
        Value::Record(layout, values) => {
            let field_names = crate::eval::fields_of_layout(*layout);
            let mut map = serde_json::Map::new();
            for (k, v) in field_names.into_iter().zip(values.iter()) {
                map.insert(k, keln_value_to_json(v));
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
            VariantPayload::Record(layout, values) => {
                let field_names = crate::eval::fields_of_layout(*layout);
                let mut map = serde_json::Map::new();
                map.insert("$tag".to_string(), json!(name));
                for (k, v) in field_names.into_iter().zip(values.iter()) {
                    map.insert(k, keln_value_to_json(v));
                }
                JValue::Object(map)
            }
        },
        Value::FnRef(n) => json!({ "$fnref": n }),
        Value::TypeRef(n) => json!({ "$typeref": n }),
        Value::Duration(ms) => json!({ "$duration_ms": ms }),
        Value::Timestamp(ms) => json!({ "$timestamp_ms": ms }),
        Value::Channel(_) => json!({ "$channel": true }),
        Value::Task(_) => json!({ "$task": "<pending>" }),
        Value::Map(map) => {
            let entries: Vec<JValue> = map
                .iter()
                .map(|(k, v)| json!([keln_value_to_json(k), keln_value_to_json(v)]))
                .collect();
            json!({ "$map": entries })
        }
        Value::Set(set) => {
            let arr: Vec<JValue> = set.iter().map(keln_value_to_json).collect();
            json!({ "$set": arr })
        }
        Value::PartialFn { name, .. } => json!({ "$partial": name }),
        Value::Closure { id } => json!({ "$closure": id }),
        Value::VmClosure { fn_idx, .. } => json!({ "$vm-closure": fn_idx }),
        Value::Queue(q) => {
            let arr: Vec<JValue> = q.iter().map(keln_value_to_json).collect();
            json!({ "$queue": arr })
        }
        Value::Heap(h) => json!({ "$heap": h.entries.len() }),
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
            Value::List(std::sync::Arc::new(arr.into_iter().map(json_to_keln_value).collect()))
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
                let mut names: Vec<String> = Vec::new();
                let mut vals: Vec<Value> = Vec::new();
                for (k, v) in map.into_iter().filter(|(k, _)| k != "$tag") {
                    names.push(k);
                    vals.push(json_to_keln_value(v));
                }

                if names.is_empty() {
                    Value::Variant { name, payload: VariantPayload::Unit }
                } else {
                    let layout = crate::eval::intern_layout(&names);
                    Value::Variant { name, payload: VariantPayload::Record(layout, vals) }
                }
            } else {
                // Plain object → Record
                let mut names: Vec<String> = Vec::with_capacity(map.len());
                let mut vals: Vec<Value> = Vec::with_capacity(map.len());
                for (k, v) in map {
                    names.push(k);
                    vals.push(json_to_keln_value(v));
                }
                let layout = crate::eval::intern_layout(&names);
                Value::Record(layout, vals)
            }
        }
    }
}
