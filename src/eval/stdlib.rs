use super::{RuntimeError, Value, VariantPayload};
use super::eval::Evaluator;

/// Return true if `name` is a built-in stdlib function.
pub fn is_stdlib(name: &str) -> bool {
    matches!(
        name,
        "Result.ok"
            | "Result.err"
            | "Result.map"
            | "Result.bind"
            | "Result.mapErr"
            | "Result.isOk"
            | "Result.isErr"
            | "Result.unwrap"
            | "Result.sequence"
            | "Maybe.some"
            | "Maybe.none"
            | "Maybe.map"
            | "Maybe.getOr"
            | "Maybe.isSome"
            | "Maybe.isNone"
            | "Maybe.bind"
            | "List.map"
            | "List.filter"
            | "List.foldl"
            | "List.foldr"
            | "List.len"
            | "List.head"
            | "List.tail"
            | "List.append"
            | "List.concat"
            | "List.isEmpty"
            | "List.find"
            | "List.contains"
            | "List.reverse"
            | "List.zip"
            | "List.flatten"
            | "List.take"
            | "List.drop"
            | "List.range"
            | "Int.parse"
            | "Int.toString"
            | "Int.abs"
            | "Int.min"
            | "Int.max"
            | "Int.clamp"
            | "Float.toString"
            | "Float.approxEq"
            | "Float.abs"
            | "Float.floor"
            | "Float.ceil"
            | "String.len"
            | "String.trim"
            | "String.concat"
            | "String.contains"
            | "String.startsWith"
            | "String.endsWith"
            | "String.toLower"
            | "String.toUpper"
            | "String.fromInt"
            | "String.slice"
            | "String.split"
            | "String.join"
            | "String.isEmpty"
            | "Bytes.len"
            | "Bytes.empty"
            | "Bytes.fromString"
            | "Bool.toString"
            | "Bool.not"
            | "Task.spawn"
            | "Task.await"
            | "Task.awaitAll"
            | "Task.sequence"
            | "Log.debug"
            | "Log.info"
            | "Log.warn"
            | "Log.error"
            | "Float.add"
            | "Float.sub"
            | "Float.multiply"
            | "Float.divide"
            | "Float.pow"
            | "Float.round"
            | "Float.toInt"
            | "Float.fromInt"
            | "Float.compare"
            | "Int.toFloat"
            | "Int.pow"
            | "Duration.ms"
            | "Duration.seconds"
            | "Duration.minutes"
            | "Duration.add"
            | "Duration.multiply"
            | "Timestamp.add"
            | "Timestamp.sub"
            | "Timestamp.compare"
            | "Timestamp.gte"
            | "Timestamp.lte"
            | "Timestamp.gt"
            | "Timestamp.lt"
            | "Timestamp.eq"
            | "Clock.now"
            | "Clock.since"
            | "Clock.after"
            | "Clock.sleep"
    )
}

/// Dispatch a stdlib call by name. `args` is the already-evaluated argument list.
pub fn dispatch(
    name: &str,
    args: Vec<Value>,
    ev: &mut Evaluator,
) -> Result<Value, RuntimeError> {
    match name {
        // =====================================================================
        // Result
        // =====================================================================
        "Result.ok" => {
            let v = one(args, "Result.ok")?;
            Ok(ok(v))
        }
        "Result.err" => {
            let v = one(args, "Result.err")?;
            Ok(err(v))
        }
        "Result.isOk" => {
            let v = one(args, "Result.isOk")?;
            Ok(Value::Bool(is_variant(&v, "Ok")))
        }
        "Result.isErr" => {
            let v = one(args, "Result.isErr")?;
            Ok(Value::Bool(is_variant(&v, "Err")))
        }
        "Result.unwrap" => {
            let v = one(args, "Result.unwrap")?;
            match v {
                Value::Variant { name, payload: VariantPayload::Tuple(inner) }
                    if name == "Ok" =>
                {
                    Ok(*inner)
                }
                _ => Err(RuntimeError::new("Result.unwrap: called on Err")),
            }
        }
        "Result.map" => {
            let (result, f) = two(args, "Result.map")?;
            match result {
                Value::Variant { name, payload: VariantPayload::Tuple(v) } if name == "Ok" => {
                    let mapped = ev.call_value(f, *v, &sp())?;
                    Ok(ok(mapped))
                }
                e @ Value::Variant { .. } => Ok(e),
                _ => Err(RuntimeError::new("Result.map: expected Result")),
            }
        }
        "Result.bind" => {
            let (result, f) = two(args, "Result.bind")?;
            match result {
                Value::Variant { name, payload: VariantPayload::Tuple(v) } if name == "Ok" => {
                    ev.call_value(f, *v, &sp())
                }
                e @ Value::Variant { .. } => Ok(e),
                _ => Err(RuntimeError::new("Result.bind: expected Result")),
            }
        }
        "Result.mapErr" => {
            let (result, f) = two(args, "Result.mapErr")?;
            match result {
                ref o @ Value::Variant { ref name, .. } if name == "Ok" => Ok(o.clone()),
                Value::Variant { name, payload: VariantPayload::Tuple(v) } if name == "Err" => {
                    let mapped = ev.call_value(f, *v, &sp())?;
                    Ok(err(mapped))
                }
                _ => Err(RuntimeError::new("Result.mapErr: expected Result")),
            }
        }
        "Result.sequence" => {
            // List<Result<T,E>> -> Result<List<T>, E>
            let v = one(args, "Result.sequence")?;
            match v {
                Value::List(items) => {
                    let mut oks = Vec::new();
                    for item in items {
                        match item {
                            Value::Variant { name, payload: VariantPayload::Tuple(inner) }
                                if name == "Ok" =>
                            {
                                oks.push(*inner);
                            }
                            ref e @ Value::Variant { ref name, .. } if name == "Err" => {
                                return Ok(e.clone());
                            }
                            other => oks.push(other),
                        }
                    }
                    Ok(ok(Value::List(oks)))
                }
                _ => Err(RuntimeError::new("Result.sequence: expected List")),
            }
        }

        // =====================================================================
        // Maybe
        // =====================================================================
        "Maybe.some" => {
            let v = one(args, "Maybe.some")?;
            Ok(some(v))
        }
        "Maybe.none" => Ok(none()),
        "Maybe.isSome" => {
            let v = one(args, "Maybe.isSome")?;
            Ok(Value::Bool(is_variant(&v, "Some")))
        }
        "Maybe.isNone" => {
            let v = one(args, "Maybe.isNone")?;
            Ok(Value::Bool(is_variant(&v, "None")))
        }
        "Maybe.map" => {
            let (maybe, f) = two(args, "Maybe.map")?;
            match maybe {
                Value::Variant { name, payload: VariantPayload::Tuple(v) } if name == "Some" => {
                    let mapped = ev.call_value(f, *v, &sp())?;
                    Ok(some(mapped))
                }
                ref n @ Value::Variant { ref name, payload: VariantPayload::Unit } if name == "None" => {
                    Ok(n.clone())
                }
                _ => Err(RuntimeError::new("Maybe.map: expected Maybe")),
            }
        }
        "Maybe.bind" => {
            let (maybe, f) = two(args, "Maybe.bind")?;
            match maybe {
                Value::Variant { name, payload: VariantPayload::Tuple(v) } if name == "Some" => {
                    ev.call_value(f, *v, &sp())
                }
                ref n @ Value::Variant { ref name, payload: VariantPayload::Unit } if name == "None" => {
                    Ok(n.clone())
                }
                _ => Err(RuntimeError::new("Maybe.bind: expected Maybe")),
            }
        }
        "Maybe.getOr" => {
            let (maybe, default) = two(args, "Maybe.getOr")?;
            match maybe {
                Value::Variant { name, payload: VariantPayload::Tuple(v) } if name == "Some" => {
                    Ok(*v)
                }
                _ => Ok(default),
            }
        }

        // =====================================================================
        // List
        // =====================================================================
        "List.map" => {
            let (list, f) = two(args, "List.map")?;
            match list {
                Value::List(items) => {
                    let mut result = Vec::with_capacity(items.len());
                    for item in items {
                        result.push(ev.call_value(f.clone(), item, &sp())?);
                    }
                    Ok(Value::List(result))
                }
                _ => Err(RuntimeError::new("List.map: expected List")),
            }
        }
        "List.filter" => {
            let (list, f) = two(args, "List.filter")?;
            match list {
                Value::List(items) => {
                    let mut result = Vec::new();
                    for item in items {
                        if ev.call_value(f.clone(), item.clone(), &sp())? == Value::Bool(true) {
                            result.push(item);
                        }
                    }
                    Ok(Value::List(result))
                }
                _ => Err(RuntimeError::new("List.filter: expected List")),
            }
        }
        "List.foldl" => {
            // foldl(list, init, fn) — fn receives { acc, item }
            let (list, init, f) = three(args, "List.foldl")?;
            match list {
                Value::List(items) => {
                    let mut acc = init;
                    for item in items {
                        let arg = Value::Record(vec![
                            ("acc".to_string(), acc),
                            ("item".to_string(), item),
                        ]);
                        acc = ev.call_value(f.clone(), arg, &sp())?;
                    }
                    Ok(acc)
                }
                _ => Err(RuntimeError::new("List.foldl: expected List")),
            }
        }
        "List.foldr" => {
            let (list, init, f) = three(args, "List.foldr")?;
            match list {
                Value::List(mut items) => {
                    items.reverse();
                    let mut acc = init;
                    for item in items {
                        let arg = Value::Record(vec![
                            ("acc".to_string(), acc),
                            ("item".to_string(), item),
                        ]);
                        acc = ev.call_value(f.clone(), arg, &sp())?;
                    }
                    Ok(acc)
                }
                _ => Err(RuntimeError::new("List.foldr: expected List")),
            }
        }
        "List.len" => {
            let v = one(args, "List.len")?;
            match v {
                Value::List(items) => Ok(Value::Int(items.len() as i64)),
                _ => Err(RuntimeError::new("List.len: expected List")),
            }
        }
        "List.isEmpty" => {
            let v = one(args, "List.isEmpty")?;
            match v {
                Value::List(items) => Ok(Value::Bool(items.is_empty())),
                _ => Err(RuntimeError::new("List.isEmpty: expected List")),
            }
        }
        "List.head" => {
            let v = one(args, "List.head")?;
            match v {
                Value::List(mut items) => {
                    if items.is_empty() {
                        Ok(none())
                    } else {
                        Ok(some(items.remove(0)))
                    }
                }
                _ => Err(RuntimeError::new("List.head: expected List")),
            }
        }
        "List.tail" => {
            let v = one(args, "List.tail")?;
            match v {
                Value::List(mut items) => {
                    if !items.is_empty() {
                        items.remove(0);
                    }
                    Ok(Value::List(items))
                }
                _ => Err(RuntimeError::new("List.tail: expected List")),
            }
        }
        "List.append" => {
            let (list, item) = two(args, "List.append")?;
            match list {
                Value::List(mut items) => {
                    items.push(item);
                    Ok(Value::List(items))
                }
                _ => Err(RuntimeError::new("List.append: expected List")),
            }
        }
        "List.concat" => {
            let (a, b) = two(args, "List.concat")?;
            match (a, b) {
                (Value::List(mut a), Value::List(b)) => {
                    a.extend(b);
                    Ok(Value::List(a))
                }
                _ => Err(RuntimeError::new("List.concat: expected two Lists")),
            }
        }
        "List.reverse" => {
            let v = one(args, "List.reverse")?;
            match v {
                Value::List(mut items) => {
                    items.reverse();
                    Ok(Value::List(items))
                }
                _ => Err(RuntimeError::new("List.reverse: expected List")),
            }
        }
        "List.find" => {
            let (list, f) = two(args, "List.find")?;
            match list {
                Value::List(items) => {
                    for item in items {
                        if ev.call_value(f.clone(), item.clone(), &sp())? == Value::Bool(true) {
                            return Ok(some(item));
                        }
                    }
                    Ok(none())
                }
                _ => Err(RuntimeError::new("List.find: expected List")),
            }
        }
        "List.contains" => {
            let (list, item) = two(args, "List.contains")?;
            match list {
                Value::List(items) => Ok(Value::Bool(items.contains(&item))),
                _ => Err(RuntimeError::new("List.contains: expected List")),
            }
        }
        "List.take" => {
            let (list, n) = two(args, "List.take")?;
            match (list, n) {
                (Value::List(items), Value::Int(n)) => {
                    Ok(Value::List(items.into_iter().take(n.max(0) as usize).collect()))
                }
                _ => Err(RuntimeError::new("List.take: expected List and Int")),
            }
        }
        "List.drop" => {
            let (list, n) = two(args, "List.drop")?;
            match (list, n) {
                (Value::List(items), Value::Int(n)) => {
                    Ok(Value::List(items.into_iter().skip(n.max(0) as usize).collect()))
                }
                _ => Err(RuntimeError::new("List.drop: expected List and Int")),
            }
        }
        "List.zip" => {
            let (a, b) = two(args, "List.zip")?;
            match (a, b) {
                (Value::List(a), Value::List(b)) => Ok(Value::List(
                    a.into_iter()
                        .zip(b)
                        .map(|(x, y)| {
                            Value::Record(vec![
                                ("_0".to_string(), x),
                                ("_1".to_string(), y),
                            ])
                        })
                        .collect(),
                )),
                _ => Err(RuntimeError::new("List.zip: expected two Lists")),
            }
        }
        "List.flatten" => {
            let v = one(args, "List.flatten")?;
            match v {
                Value::List(items) => {
                    let mut result = Vec::new();
                    for item in items {
                        match item {
                            Value::List(inner) => result.extend(inner),
                            other => result.push(other),
                        }
                    }
                    Ok(Value::List(result))
                }
                _ => Err(RuntimeError::new("List.flatten: expected List")),
            }
        }
        "List.range" => {
            // range(start, end_exclusive) -> List<Int>
            let (start, end) = two(args, "List.range")?;
            match (start, end) {
                (Value::Int(s), Value::Int(e)) => {
                    Ok(Value::List((s..e).map(Value::Int).collect()))
                }
                _ => Err(RuntimeError::new("List.range: expected two Ints")),
            }
        }

        // =====================================================================
        // Int
        // =====================================================================
        "Int.parse" => {
            let v = one(args, "Int.parse")?;
            match v {
                Value::Str(s) => match s.trim().parse::<i64>() {
                    Ok(n) => Ok(ok(Value::Int(n))),
                    Err(_) => Ok(err(Value::Variant {
                        name: "NotANumber".to_string(),
                        payload: VariantPayload::Record(vec![(
                            "input".to_string(),
                            Value::Str(s),
                        )]),
                    })),
                },
                _ => Err(RuntimeError::new("Int.parse: expected String")),
            }
        }
        "Int.toString" | "Int.fromInt" => {
            let v = one(args, name)?;
            match v {
                Value::Int(n) => Ok(Value::Str(n.to_string())),
                _ => Err(RuntimeError::new(format!("{}: expected Int", name))),
            }
        }
        "Int.abs" => {
            let v = one(args, "Int.abs")?;
            match v {
                Value::Int(n) => Ok(Value::Int(n.abs())),
                _ => Err(RuntimeError::new("Int.abs: expected Int")),
            }
        }
        "Int.min" => {
            let (a, b) = two(args, "Int.min")?;
            match (a, b) {
                (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a.min(b))),
                _ => Err(RuntimeError::new("Int.min: expected two Ints")),
            }
        }
        "Int.max" => {
            let (a, b) = two(args, "Int.max")?;
            match (a, b) {
                (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a.max(b))),
                _ => Err(RuntimeError::new("Int.max: expected two Ints")),
            }
        }
        "Int.clamp" => {
            let (v, lo, hi) = three(args, "Int.clamp")?;
            match (v, lo, hi) {
                (Value::Int(v), Value::Int(lo), Value::Int(hi)) => {
                    Ok(Value::Int(v.clamp(lo, hi)))
                }
                _ => Err(RuntimeError::new("Int.clamp: expected three Ints")),
            }
        }

        // =====================================================================
        // Float
        // =====================================================================
        "Float.toString" => {
            let v = one(args, "Float.toString")?;
            match v {
                Value::Float(f) => Ok(Value::Str(f.to_string())),
                _ => Err(RuntimeError::new("Float.toString: expected Float")),
            }
        }
        "Float.approxEq" => {
            let (a, b) = two(args, "Float.approxEq")?;
            match (a, b) {
                (Value::Float(a), Value::Float(b)) => Ok(Value::Bool((a - b).abs() < 1e-9)),
                _ => Err(RuntimeError::new("Float.approxEq: expected two Floats")),
            }
        }
        "Float.abs" => {
            let v = one(args, "Float.abs")?;
            match v {
                Value::Float(f) => Ok(Value::Float(f.abs())),
                _ => Err(RuntimeError::new("Float.abs: expected Float")),
            }
        }
        "Float.floor" => {
            let v = one(args, "Float.floor")?;
            match v {
                Value::Float(f) => Ok(Value::Float(f.floor())),
                _ => Err(RuntimeError::new("Float.floor: expected Float")),
            }
        }
        "Float.ceil" => {
            let v = one(args, "Float.ceil")?;
            match v {
                Value::Float(f) => Ok(Value::Float(f.ceil())),
                _ => Err(RuntimeError::new("Float.ceil: expected Float")),
            }
        }

        // =====================================================================
        // String
        // =====================================================================
        "String.len" => {
            let v = one(args, "String.len")?;
            match v {
                Value::Str(s) => Ok(Value::Int(s.len() as i64)),
                _ => Err(RuntimeError::new("String.len: expected String")),
            }
        }
        "String.trim" => {
            let v = one(args, "String.trim")?;
            match v {
                Value::Str(s) => Ok(Value::Str(s.trim().to_string())),
                _ => Err(RuntimeError::new("String.trim: expected String")),
            }
        }
        "String.isEmpty" => {
            let v = one(args, "String.isEmpty")?;
            match v {
                Value::Str(s) => Ok(Value::Bool(s.is_empty())),
                _ => Err(RuntimeError::new("String.isEmpty: expected String")),
            }
        }
        "String.concat" => {
            let (a, b) = two(args, "String.concat")?;
            match (a, b) {
                (Value::Str(a), Value::Str(b)) => Ok(Value::Str(format!("{}{}", a, b))),
                _ => Err(RuntimeError::new("String.concat: expected two Strings")),
            }
        }
        "String.contains" => {
            let (s, sub) = two(args, "String.contains")?;
            match (s, sub) {
                (Value::Str(s), Value::Str(sub)) => Ok(Value::Bool(s.contains(sub.as_str()))),
                _ => Err(RuntimeError::new("String.contains: expected two Strings")),
            }
        }
        "String.startsWith" => {
            let (s, p) = two(args, "String.startsWith")?;
            match (s, p) {
                (Value::Str(s), Value::Str(p)) => Ok(Value::Bool(s.starts_with(p.as_str()))),
                _ => Err(RuntimeError::new("String.startsWith: expected two Strings")),
            }
        }
        "String.endsWith" => {
            let (s, p) = two(args, "String.endsWith")?;
            match (s, p) {
                (Value::Str(s), Value::Str(p)) => Ok(Value::Bool(s.ends_with(p.as_str()))),
                _ => Err(RuntimeError::new("String.endsWith: expected two Strings")),
            }
        }
        "String.toLower" => {
            let v = one(args, "String.toLower")?;
            match v {
                Value::Str(s) => Ok(Value::Str(s.to_lowercase())),
                _ => Err(RuntimeError::new("String.toLower: expected String")),
            }
        }
        "String.toUpper" => {
            let v = one(args, "String.toUpper")?;
            match v {
                Value::Str(s) => Ok(Value::Str(s.to_uppercase())),
                _ => Err(RuntimeError::new("String.toUpper: expected String")),
            }
        }
        "String.fromInt" => {
            let v = one(args, "String.fromInt")?;
            match v {
                Value::Int(n) => Ok(Value::Str(n.to_string())),
                _ => Err(RuntimeError::new("String.fromInt: expected Int")),
            }
        }
        "String.slice" => {
            let (s, start, end) = three(args, "String.slice")?;
            match (s, start, end) {
                (Value::Str(s), Value::Int(start), Value::Int(end)) => {
                    let chars: Vec<char> = s.chars().collect();
                    let s = start.max(0) as usize;
                    let e = (end as usize).min(chars.len());
                    if s <= e {
                        Ok(Value::Str(chars[s..e].iter().collect()))
                    } else {
                        Ok(Value::Str(String::new()))
                    }
                }
                _ => Err(RuntimeError::new("String.slice: expected String, Int, Int")),
            }
        }
        "String.split" => {
            let (s, sep) = two(args, "String.split")?;
            match (s, sep) {
                (Value::Str(s), Value::Str(sep)) => Ok(Value::List(
                    s.split(sep.as_str())
                        .map(|p| Value::Str(p.to_string()))
                        .collect(),
                )),
                _ => Err(RuntimeError::new("String.split: expected String, String")),
            }
        }
        "String.join" => {
            let (list, sep) = two(args, "String.join")?;
            match (list, sep) {
                (Value::List(items), Value::Str(sep)) => {
                    let parts: Result<Vec<_>, _> = items
                        .iter()
                        .map(|v| match v {
                            Value::Str(s) => Ok(s.as_str()),
                            _ => Err(RuntimeError::new(
                                "String.join: list elements must be Strings",
                            )),
                        })
                        .collect();
                    Ok(Value::Str(parts?.join(sep.as_str())))
                }
                _ => Err(RuntimeError::new("String.join: expected List and String")),
            }
        }

        // =====================================================================
        // Bytes
        // =====================================================================
        "Bytes.len" => {
            let v = one(args, "Bytes.len")?;
            match v {
                Value::Bytes(b) => Ok(Value::Int(b.len() as i64)),
                _ => Err(RuntimeError::new("Bytes.len: expected Bytes")),
            }
        }
        "Bytes.empty" => Ok(Value::Bytes(vec![])),
        "Bytes.fromString" => {
            let v = one(args, "Bytes.fromString")?;
            match v {
                Value::Str(s) => Ok(Value::Bytes(s.into_bytes())),
                _ => Err(RuntimeError::new("Bytes.fromString: expected String")),
            }
        }

        // =====================================================================
        // Bool
        // =====================================================================
        "Bool.toString" => {
            let v = one(args, "Bool.toString")?;
            match v {
                Value::Bool(b) => Ok(Value::Str(b.to_string())),
                _ => Err(RuntimeError::new("Bool.toString: expected Bool")),
            }
        }
        "Bool.not" => {
            let v = one(args, "Bool.not")?;
            match v {
                Value::Bool(b) => Ok(Value::Bool(!b)),
                _ => Err(RuntimeError::new("Bool.not: expected Bool")),
            }
        }

        // =====================================================================
        // Task (sync stubs — no real async in this evaluator)
        // =====================================================================
        "Task.spawn" => {
            let v = one(args, "Task.spawn")?;
            let result = match &v {
                Value::FnRef(name) => ev.call_fn(name, Value::Unit)?,
                Value::PartialFn { name, bound } => ev.call_fn(name, Value::Record(bound.clone()))?,
                _ => v.clone(),
            };
            Ok(Value::Variant {
                name: "Task".to_string(),
                payload: VariantPayload::Tuple(Box::new(result)),
            })
        }
        "Task.await" => {
            let v = one(args, "Task.await")?;
            match v {
                Value::Variant { name, payload: VariantPayload::Tuple(inner) }
                    if name == "Task" =>
                {
                    Ok(*inner)
                }
                other => Ok(other),
            }
        }
        "Task.awaitAll" => {
            let v = one(args, "Task.awaitAll")?;
            match v {
                Value::List(tasks) => Ok(Value::List(
                    tasks
                        .into_iter()
                        .map(|t| match t {
                            Value::Variant { name, payload: VariantPayload::Tuple(inner) }
                                if name == "Task" =>
                            {
                                *inner
                            }
                            other => other,
                        })
                        .collect(),
                )),
                _ => Err(RuntimeError::new("Task.awaitAll: expected List")),
            }
        }
        "Task.sequence" => {
            // List<Result<T,E>> -> Result<List<T>, E>
            dispatch("Result.sequence", args, ev)
        }

        // =====================================================================
        // Log (IO effect — prints to stdout in the sync model)
        // =====================================================================
        "Log.debug" | "Log.info" | "Log.warn" | "Log.error" => {
            let v = one(args, name)?;
            let level = name.split('.').nth(1).unwrap_or("log");
            println!("[{}] {}", level.to_uppercase(), v);
            Ok(Value::Unit)
        }

        // =====================================================================
        // Float (complete arithmetic)
        // =====================================================================
        "Float.add" => {
            let (a, b) = two(args, "Float.add")?;
            match (a, b) {
                (Value::Float(x), Value::Float(y)) => Ok(Value::Float(x + y)),
                _ => Err(RuntimeError::new("Float.add: expected Float, Float")),
            }
        }
        "Float.sub" => {
            let (a, b) = two(args, "Float.sub")?;
            match (a, b) {
                (Value::Float(x), Value::Float(y)) => Ok(Value::Float(x - y)),
                _ => Err(RuntimeError::new("Float.sub: expected Float, Float")),
            }
        }
        "Float.multiply" => {
            let (a, b) = two(args, "Float.multiply")?;
            match (a, b) {
                (Value::Float(x), Value::Float(y)) => Ok(Value::Float(x * y)),
                _ => Err(RuntimeError::new("Float.multiply: expected Float, Float")),
            }
        }
        "Float.divide" => {
            let (a, b) = two(args, "Float.divide")?;
            match (a, b) {
                (Value::Float(x), Value::Float(y)) => {
                    if y == 0.0 {
                        Err(RuntimeError::new("Float.divide: division by zero"))
                    } else {
                        Ok(Value::Float(x / y))
                    }
                }
                _ => Err(RuntimeError::new("Float.divide: expected Float, Float")),
            }
        }
        "Float.pow" => {
            let (a, b) = two(args, "Float.pow")?;
            match (a, b) {
                (Value::Float(x), Value::Float(y)) => Ok(Value::Float(x.powf(y))),
                _ => Err(RuntimeError::new("Float.pow: expected Float, Float")),
            }
        }
        "Float.round" => {
            let v = one(args, "Float.round")?;
            match v {
                Value::Float(f) => Ok(Value::Float(f.round())),
                _ => Err(RuntimeError::new("Float.round: expected Float")),
            }
        }
        "Float.toInt" => {
            let v = one(args, "Float.toInt")?;
            match v {
                Value::Float(f) => Ok(Value::Int(f.trunc() as i64)),
                _ => Err(RuntimeError::new("Float.toInt: expected Float")),
            }
        }
        "Float.fromInt" => {
            let v = one(args, "Float.fromInt")?;
            match v {
                Value::Int(n) => Ok(Value::Float(n as f64)),
                _ => Err(RuntimeError::new("Float.fromInt: expected Int")),
            }
        }
        "Float.compare" => {
            let (a, b) = two(args, "Float.compare")?;
            match (a, b) {
                (Value::Float(x), Value::Float(y)) => Ok(ordering(x.partial_cmp(&y))),
                _ => Err(RuntimeError::new("Float.compare: expected Float, Float")),
            }
        }

        // =====================================================================
        // Int additions
        // =====================================================================
        "Int.toFloat" => {
            let v = one(args, "Int.toFloat")?;
            match v {
                Value::Int(n) => Ok(Value::Float(n as f64)),
                _ => Err(RuntimeError::new("Int.toFloat: expected Int")),
            }
        }
        "Int.pow" => {
            let (a, b) = two(args, "Int.pow")?;
            match (a, b) {
                (Value::Int(base), Value::Int(exp)) if exp >= 0 => {
                    Ok(Value::Int(base.pow(exp as u32)))
                }
                (Value::Int(_), Value::Int(exp)) => Err(RuntimeError::new(format!(
                    "Int.pow: exponent must be >= 0, got {}",
                    exp
                ))),
                _ => Err(RuntimeError::new("Int.pow: expected Int, Int")),
            }
        }

        // =====================================================================
        // Duration
        // =====================================================================
        "Duration.ms" => {
            let v = one(args, "Duration.ms")?;
            match v {
                Value::Int(n) => Ok(Value::Duration(n)),
                _ => Err(RuntimeError::new("Duration.ms: expected Int")),
            }
        }
        "Duration.seconds" => {
            let v = one(args, "Duration.seconds")?;
            match v {
                Value::Int(n) => Ok(Value::Duration(n * 1_000)),
                _ => Err(RuntimeError::new("Duration.seconds: expected Int")),
            }
        }
        "Duration.minutes" => {
            let v = one(args, "Duration.minutes")?;
            match v {
                Value::Int(n) => Ok(Value::Duration(n * 60_000)),
                _ => Err(RuntimeError::new("Duration.minutes: expected Int")),
            }
        }
        "Duration.add" => {
            let (a, b) = two(args, "Duration.add")?;
            match (a, b) {
                (Value::Duration(x), Value::Duration(y)) => Ok(Value::Duration(x + y)),
                _ => Err(RuntimeError::new("Duration.add: expected Duration, Duration")),
            }
        }
        "Duration.multiply" => {
            let (a, b) = two(args, "Duration.multiply")?;
            match (a, b) {
                (Value::Duration(d), Value::Int(n)) => Ok(Value::Duration(d * n)),
                _ => Err(RuntimeError::new("Duration.multiply: expected Duration, Int")),
            }
        }

        // =====================================================================
        // Timestamp
        // =====================================================================
        "Timestamp.add" => {
            let (a, b) = two(args, "Timestamp.add")?;
            match (a, b) {
                (Value::Timestamp(ts), Value::Duration(d)) => Ok(Value::Timestamp(ts + d)),
                _ => Err(RuntimeError::new("Timestamp.add: expected Timestamp, Duration")),
            }
        }
        "Timestamp.sub" => {
            let (a, b) = two(args, "Timestamp.sub")?;
            match (a, b) {
                (Value::Timestamp(a), Value::Timestamp(b)) => Ok(Value::Duration(a - b)),
                _ => Err(RuntimeError::new("Timestamp.sub: expected Timestamp, Timestamp")),
            }
        }
        "Timestamp.compare" => {
            let (a, b) = two(args, "Timestamp.compare")?;
            match (a, b) {
                (Value::Timestamp(x), Value::Timestamp(y)) => Ok(ordering(x.partial_cmp(&y))),
                _ => Err(RuntimeError::new("Timestamp.compare: expected Timestamp, Timestamp")),
            }
        }
        "Timestamp.gte" => {
            let (a, b) = two(args, "Timestamp.gte")?;
            match (a, b) {
                (Value::Timestamp(x), Value::Timestamp(y)) => Ok(Value::Bool(x >= y)),
                _ => Err(RuntimeError::new("Timestamp.gte: expected Timestamp, Timestamp")),
            }
        }
        "Timestamp.lte" => {
            let (a, b) = two(args, "Timestamp.lte")?;
            match (a, b) {
                (Value::Timestamp(x), Value::Timestamp(y)) => Ok(Value::Bool(x <= y)),
                _ => Err(RuntimeError::new("Timestamp.lte: expected Timestamp, Timestamp")),
            }
        }
        "Timestamp.gt" => {
            let (a, b) = two(args, "Timestamp.gt")?;
            match (a, b) {
                (Value::Timestamp(x), Value::Timestamp(y)) => Ok(Value::Bool(x > y)),
                _ => Err(RuntimeError::new("Timestamp.gt: expected Timestamp, Timestamp")),
            }
        }
        "Timestamp.lt" => {
            let (a, b) = two(args, "Timestamp.lt")?;
            match (a, b) {
                (Value::Timestamp(x), Value::Timestamp(y)) => Ok(Value::Bool(x < y)),
                _ => Err(RuntimeError::new("Timestamp.lt: expected Timestamp, Timestamp")),
            }
        }
        "Timestamp.eq" => {
            let (a, b) = two(args, "Timestamp.eq")?;
            match (a, b) {
                (Value::Timestamp(x), Value::Timestamp(y)) => Ok(Value::Bool(x == y)),
                _ => Err(RuntimeError::new("Timestamp.eq: expected Timestamp, Timestamp")),
            }
        }

        // =====================================================================
        // Clock
        // =====================================================================
        "Clock.now" => {
            Ok(Value::Timestamp(now_millis()))
        }
        "Clock.since" => {
            let v = one(args, "Clock.since")?;
            match v {
                Value::Timestamp(ts) => Ok(Value::Duration(now_millis() - ts)),
                _ => Err(RuntimeError::new("Clock.since: expected Timestamp")),
            }
        }
        "Clock.after" => {
            let v = one(args, "Clock.after")?;
            match v {
                Value::Duration(d) => Ok(Value::Timestamp(now_millis() + d)),
                _ => Err(RuntimeError::new("Clock.after: expected Duration")),
            }
        }
        "Clock.sleep" => {
            Ok(Value::Unit)
        }

        _ => Err(RuntimeError::new(format!("unknown stdlib function '{}'", name))),
    }
}

// =========================================================================
// Arg extraction helpers
// =========================================================================

fn one(mut args: Vec<Value>, fn_name: &str) -> Result<Value, RuntimeError> {
    match args.len() {
        0 => Err(RuntimeError::new(format!("{}: expected 1 argument, got 0", fn_name))),
        1 => Ok(args.remove(0)),
        _ => Ok(args.remove(0)), // pipeline may prepend extra; take first
    }
}

fn two(mut args: Vec<Value>, fn_name: &str) -> Result<(Value, Value), RuntimeError> {
    if args.len() >= 2 {
        let a = args.remove(0);
        let b = args.remove(0);
        Ok((a, b))
    } else if args.len() == 1 {
        match args.remove(0) {
            Value::Record(mut fields) if fields.len() >= 2 => {
                let (_, a) = fields.remove(0);
                let (_, b) = fields.remove(0);
                Ok((a, b))
            }
            other => Err(RuntimeError::new(format!(
                "{}: expected 2 arguments, got single value: {}",
                fn_name, other
            ))),
        }
    } else {
        Err(RuntimeError::new(format!("{}: expected 2 arguments, got 0", fn_name)))
    }
}

fn three(mut args: Vec<Value>, fn_name: &str) -> Result<(Value, Value, Value), RuntimeError> {
    if args.len() >= 3 {
        let a = args.remove(0);
        let b = args.remove(0);
        let c = args.remove(0);
        Ok((a, b, c))
    } else if args.len() == 1 {
        match args.remove(0) {
            Value::Record(mut fields) if fields.len() >= 3 => {
                let (_, a) = fields.remove(0);
                let (_, b) = fields.remove(0);
                let (_, c) = fields.remove(0);
                Ok((a, b, c))
            }
            _ => Err(RuntimeError::new(format!("{}: expected 3 arguments", fn_name))),
        }
    } else {
        Err(RuntimeError::new(format!(
            "{}: expected 3 arguments, got {}",
            fn_name,
            args.len()
        )))
    }
}

// =========================================================================
// Value constructors
// =========================================================================

fn ok(v: Value) -> Value {
    Value::Variant { name: "Ok".to_string(), payload: VariantPayload::Tuple(Box::new(v)) }
}

fn err(v: Value) -> Value {
    Value::Variant { name: "Err".to_string(), payload: VariantPayload::Tuple(Box::new(v)) }
}

fn some(v: Value) -> Value {
    Value::Variant { name: "Some".to_string(), payload: VariantPayload::Tuple(Box::new(v)) }
}

fn none() -> Value {
    Value::Variant { name: "None".to_string(), payload: VariantPayload::Unit }
}

fn is_variant(v: &Value, name: &str) -> bool {
    matches!(v, Value::Variant { name: n, .. } if n == name)
}

fn sp() -> crate::ast::Span {
    crate::ast::Span { line: 0, column: 0 }
}

fn ordering(ord: Option<std::cmp::Ordering>) -> Value {
    match ord {
        Some(std::cmp::Ordering::Less) => {
            Value::Variant { name: "LessThan".to_string(), payload: VariantPayload::Unit }
        }
        Some(std::cmp::Ordering::Equal) => {
            Value::Variant { name: "Equal".to_string(), payload: VariantPayload::Unit }
        }
        Some(std::cmp::Ordering::Greater) | None => {
            Value::Variant { name: "GreaterThan".to_string(), payload: VariantPayload::Unit }
        }
    }
}

fn now_millis() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
