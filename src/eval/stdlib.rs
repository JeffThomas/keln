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
            | "Result.unwrapOr"
            | "Result.sequence"
            | "Result.toMaybe"
            | "Maybe.some"
            | "Maybe.none"
            | "Maybe.map"
            | "Maybe.getOr"
            | "Maybe.isSome"
            | "Maybe.isNone"
            | "Maybe.bind"
            | "Maybe.require"
            | "Maybe.unwrapOr"
            | "List.map"
            | "List.filter"
            | "List.foldl"
            | "List.foldr"
            | "List.len"
            | "List.length"
            | "List.head"
            | "List.tail"
            | "List.append"
            | "List.prepend"
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
            | "List.fold"
            | "List.sequence"
            | "List.repeat"
            | "List.clone"
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
            | "Float.parse"
            | "String.len"
            | "String.length"
            | "String.trim"
            | "String.concat"
            | "String.contains"
            | "String.startsWith"
            | "String.endsWith"
            | "String.toLower"
            | "String.lowercase"
            | "String.toUpper"
            | "String.uppercase"
            | "String.fromInt"
            | "String.slice"
            | "String.split"
            | "String.join"
            | "String.isEmpty"
            | "String.trimStart"
            | "String.trimEnd"
            | "String.chars"
            | "String.indexOf"
            | "String.replace"
            | "String.toString"
            | "Bytes.len"
            | "Bytes.empty"
            | "Bytes.fromString"
            | "Bytes.toString"
            | "Bool.toString"
            | "Bool.not"
            | "Bool.and"
            | "Bool.or"
            | "Task.spawn"
            | "Task.await"
            | "Task.awaitAll"
            | "Task.awaitFirst"
            | "Task.race"
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
            | "Map.empty"
            | "Map.insert"
            | "Map.get"
            | "Map.remove"
            | "Map.contains"
            | "Map.keys"
            | "Map.values"
            | "Map.toList"
            | "Map.fromList"
            | "Map.size"
            | "Map.merge"
            | "Set.empty"
            | "Set.insert"
            | "Set.contains"
            | "Set.remove"
            | "Set.toList"
            | "Set.fromList"
            | "Set.union"
            | "Set.intersect"
            | "Set.difference"
            | "Set.size"
            | "Env.get"
            | "Env.require"
            | "Json.parse"
            | "JSON.parse"
            | "Json.serialize"
            | "JSON.serialize"
            | "Http.get"
            | "Http.post"
            | "Http.put"
            | "Http.delete"
            | "Http.patch"
            | "HttpServer.start"
            | "Response.json"
            | "Response.err"
            | "GraphQL.execute"
            | "GraphQL.query"
            | "File.read"
            | "File.readLines"
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
        "Result.unwrapOr" => {
            let (result, default) = two(args, "Result.unwrapOr")?;
            match result {
                Value::Variant { name, payload: VariantPayload::Tuple(inner) } if name == "Ok" => {
                    Ok(*inner)
                }
                _ => Ok(default),
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
        "Result.toMaybe" => {
            let v = one(args, "Result.toMaybe")?;
            match v {
                Value::Variant { name, payload } if name == "Ok" => match payload {
                    VariantPayload::Tuple(inner) => Ok(some(*inner)),
                    _ => Ok(some(Value::Unit)),
                },
                Value::Variant { .. } => Ok(none()),
                _ => Err(RuntimeError::new("Result.toMaybe: expected Result")),
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
        "Maybe.getOr" | "Maybe.unwrapOr" => {
            let fname = name;
            let (maybe, default) = two(args, fname)?;
            match maybe {
                Value::Variant { name, payload: VariantPayload::Tuple(v) } if name == "Some" => {
                    Ok(*v)
                }
                _ => Ok(default),
            }
        }
        "Maybe.require" => {
            let (maybe, e) = two(args, "Maybe.require")?;
            match maybe {
                Value::Variant { name, payload: VariantPayload::Tuple(v) } if name == "Some" => {
                    Ok(ok(*v))
                }
                _ => Ok(err(e)),
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
        "List.len" | "List.length" => {
            let v = one(args, name)?;
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
        "List.prepend" => {
            let (list, item) = two(args, "List.prepend")?;
            match list {
                Value::List(mut items) => {
                    items.insert(0, item);
                    Ok(Value::List(items))
                }
                _ => Err(RuntimeError::new("List.prepend: expected List")),
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
        "List.fold" => {
            // fold(list, init, fn) — fn receives { acc: U, item: T } record
            let (list, init, f) = three(args, "List.fold")?;
            match list {
                Value::List(items) => {
                    let mut acc = init;
                    for item in items {
                        let record = Value::Record(vec![
                            ("acc".to_string(), acc),
                            ("item".to_string(), item),
                        ]);
                        acc = ev.call_value(f.clone(), record, &sp())?;
                    }
                    Ok(acc)
                }
                _ => Err(RuntimeError::new("List.fold: expected List")),
            }
        }
        "List.sequence" => {
            // List<Result<T,E>> -> Result<List<T>, E>
            dispatch("Result.sequence", args, ev)
        }
        "List.repeat" => {
            // repeat(value, count) -> List<T>
            let (value, count) = two(args, "List.repeat")?;
            match count {
                Value::Int(n) if n >= 0 => {
                    Ok(Value::List(vec![value; n as usize]))
                }
                Value::Int(n) => Err(RuntimeError::new(format!(
                    "List.repeat: count must be >= 0, got {}", n
                ))),
                _ => Err(RuntimeError::new("List.repeat: expected Int count")),
            }
        }
        "List.clone" => {
            let v = one(args, "List.clone")?;
            Ok(v)
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
        "Float.parse" => {
            let v = one(args, "Float.parse")?;
            match v {
                Value::Str(s) => match s.trim().parse::<f64>() {
                    Ok(f) => Ok(ok(Value::Float(f))),
                    Err(_) => Ok(err(Value::Str(format!("not a float: {}", s)))),
                },
                _ => Err(RuntimeError::new("Float.parse: expected String")),
            }
        }

        // =====================================================================
        // String
        // =====================================================================
        "String.len" | "String.length" => {
            let v = one(args, name)?;
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
        "String.toLower" | "String.lowercase" => {
            let v = one(args, name)?;
            match v {
                Value::Str(s) => Ok(Value::Str(s.to_lowercase())),
                _ => Err(RuntimeError::new("String.toLower: expected String")),
            }
        }
        "String.toUpper" | "String.uppercase" => {
            let v = one(args, name)?;
            match v {
                Value::Str(s) => Ok(Value::Str(s.to_uppercase())),
                _ => Err(RuntimeError::new("String.toUpper: expected String")),
            }
        }
        "String.toString" => {
            let v = one(args, "String.toString")?;
            match v {
                Value::Str(s) => Ok(Value::Str(s)),
                _ => Err(RuntimeError::new("String.toString: expected String")),
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
        "String.trimStart" => {
            let v = one(args, "String.trimStart")?;
            match v {
                Value::Str(s) => Ok(Value::Str(s.trim_start().to_string())),
                _ => Err(RuntimeError::new("String.trimStart: expected String")),
            }
        }
        "String.trimEnd" => {
            let v = one(args, "String.trimEnd")?;
            match v {
                Value::Str(s) => Ok(Value::Str(s.trim_end().to_string())),
                _ => Err(RuntimeError::new("String.trimEnd: expected String")),
            }
        }
        "String.chars" => {
            let v = one(args, "String.chars")?;
            match v {
                Value::Str(s) => Ok(Value::List(
                    s.chars().map(|c| Value::Str(c.to_string())).collect(),
                )),
                _ => Err(RuntimeError::new("String.chars: expected String")),
            }
        }
        "String.indexOf" => {
            let (s, sub) = two(args, "String.indexOf")?;
            match (s, sub) {
                (Value::Str(s), Value::Str(sub)) => {
                    let result = s.find(sub.as_str()).map(|byte_pos| {
                        s[..byte_pos].chars().count() as i64
                    });
                    match result {
                        Some(i) => Ok(some(Value::Int(i))),
                        None    => Ok(none()),
                    }
                }
                _ => Err(RuntimeError::new("String.indexOf: expected String, String")),
            }
        }
        "String.replace" => {
            let (s, from, to) = three(args, "String.replace")?;
            match (s, from, to) {
                (Value::Str(s), Value::Str(from), Value::Str(to)) => {
                    Ok(Value::Str(s.replace(from.as_str(), to.as_str())))
                }
                _ => Err(RuntimeError::new("String.replace: expected String, String, String")),
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
        "Bytes.toString" => {
            let v = one(args, "Bytes.toString")?;
            match v {
                Value::Bytes(b) => match String::from_utf8(b) {
                    Ok(s) => Ok(Value::Str(s)),
                    Err(_) => Err(RuntimeError::new("Bytes.toString: bytes are not valid UTF-8")),
                },
                _ => Err(RuntimeError::new("Bytes.toString: expected Bytes")),
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
        "Bool.and" => {
            let (a, b) = two(args, "Bool.and")?;
            match (a, b) {
                (Value::Bool(x), Value::Bool(y)) => Ok(Value::Bool(x && y)),
                _ => Err(RuntimeError::new("Bool.and: expected Bool, Bool")),
            }
        }
        "Bool.or" => {
            let (a, b) = two(args, "Bool.or")?;
            match (a, b) {
                (Value::Bool(x), Value::Bool(y)) => Ok(Value::Bool(x || y)),
                _ => Err(RuntimeError::new("Bool.or: expected Bool, Bool")),
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
            Ok(Value::Task(Box::new(result)))
        }
        "Task.await" => {
            let v = one(args, "Task.await")?;
            match v {
                Value::Task(inner) => Ok(*inner),
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
                            Value::Task(inner) => *inner,
                            other => other,
                        })
                        .collect(),
                )),
                _ => Err(RuntimeError::new("Task.awaitAll: expected List")),
            }
        }
        "Task.awaitFirst" | "Task.race" => {
            let v = one(args, name)?;
            match v {
                Value::List(mut tasks) if !tasks.is_empty() => match tasks.remove(0) {
                    Value::Task(inner) => Ok(*inner),
                    other => Ok(other),
                },
                Value::List(_) => Err(RuntimeError::new(format!("{}: list is empty", name))),
                _ => Err(RuntimeError::new(format!("{}: expected List<Task>", name))),
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

        // =====================================================================
        // Map<K,V>
        // =====================================================================
        "Map.empty" => Ok(Value::Map(std::collections::BTreeMap::new())),
        "Map.size" => {
            let v = one(args, "Map.size")?;
            match v {
                Value::Map(map) => Ok(Value::Int(map.len() as i64)),
                _ => Err(RuntimeError::new("Map.size: expected Map")),
            }
        }
        "Map.insert" => {
            let (map, key, val) = three(args, "Map.insert")?;
            match map {
                Value::Map(mut map) => {
                    map.insert(key, val);
                    Ok(Value::Map(map))
                }
                _ => Err(RuntimeError::new("Map.insert: expected Map as first arg")),
            }
        }
        "Map.get" => {
            let (map, key) = two(args, "Map.get")?;
            match map {
                Value::Map(map) => match map.get(&key) {
                    Some(v) => Ok(some(v.clone())),
                    None => Ok(none()),
                },
                _ => Err(RuntimeError::new("Map.get: expected Map")),
            }
        }
        "Map.remove" => {
            let (map, key) = two(args, "Map.remove")?;
            match map {
                Value::Map(mut map) => {
                    map.remove(&key);
                    Ok(Value::Map(map))
                }
                _ => Err(RuntimeError::new("Map.remove: expected Map")),
            }
        }
        "Map.contains" => {
            let (map, key) = two(args, "Map.contains")?;
            match map {
                Value::Map(map) => Ok(Value::Bool(map.contains_key(&key))),
                _ => Err(RuntimeError::new("Map.contains: expected Map")),
            }
        }
        "Map.keys" => {
            let v = one(args, "Map.keys")?;
            match v {
                Value::Map(map) => Ok(Value::List(map.into_keys().collect())),
                _ => Err(RuntimeError::new("Map.keys: expected Map")),
            }
        }
        "Map.values" => {
            let v = one(args, "Map.values")?;
            match v {
                Value::Map(map) => Ok(Value::List(map.into_values().collect())),
                _ => Err(RuntimeError::new("Map.values: expected Map")),
            }
        }
        "Map.toList" => {
            let v = one(args, "Map.toList")?;
            match v {
                Value::Map(map) => Ok(Value::List(
                    map.into_iter().map(|(k, v)| {
                        Value::Record(vec![("key".to_string(), k), ("value".to_string(), v)])
                    }).collect()
                )),
                _ => Err(RuntimeError::new("Map.toList: expected Map")),
            }
        }
        "Map.fromList" => {
            let v = one(args, "Map.fromList")?;
            match v {
                Value::List(items) => {
                    let mut map = std::collections::BTreeMap::new();
                    for item in items {
                        match item {
                            Value::Record(mut fields) if fields.len() >= 2 => {
                                let (_, key) = fields.remove(0);
                                let (_, val) = fields.remove(0);
                                map.insert(key, val);
                            }
                            _ => return Err(RuntimeError::new("Map.fromList: each item must be {key, value}")),
                        }
                    }
                    Ok(Value::Map(map))
                }
                _ => Err(RuntimeError::new("Map.fromList: expected List")),
            }
        }
        "Map.merge" => {
            let (a, b) = two(args, "Map.merge")?;
            match (a, b) {
                (Value::Map(mut map_a), Value::Map(map_b)) => {
                    map_a.extend(map_b);
                    Ok(Value::Map(map_a))
                }
                _ => Err(RuntimeError::new("Map.merge: expected two Maps")),
            }
        }

        // =====================================================================
        // Set<T>
        // =====================================================================
        "Set.empty" => Ok(Value::Set(std::collections::BTreeSet::new())),
        "Set.size" => {
            let v = one(args, "Set.size")?;
            match v {
                Value::Set(set) => Ok(Value::Int(set.len() as i64)),
                _ => Err(RuntimeError::new("Set.size: expected Set")),
            }
        }
        "Set.insert" => {
            let (set, item) = two(args, "Set.insert")?;
            match set {
                Value::Set(mut set) => {
                    set.insert(item);
                    Ok(Value::Set(set))
                }
                _ => Err(RuntimeError::new("Set.insert: expected Set")),
            }
        }
        "Set.contains" => {
            let (set, item) = two(args, "Set.contains")?;
            match set {
                Value::Set(set) => Ok(Value::Bool(set.contains(&item))),
                _ => Err(RuntimeError::new("Set.contains: expected Set")),
            }
        }
        "Set.remove" => {
            let (set, item) = two(args, "Set.remove")?;
            match set {
                Value::Set(mut set) => {
                    set.remove(&item);
                    Ok(Value::Set(set))
                }
                _ => Err(RuntimeError::new("Set.remove: expected Set")),
            }
        }
        "Set.toList" => {
            let v = one(args, "Set.toList")?;
            match v {
                Value::Set(set) => Ok(Value::List(set.into_iter().collect())),
                _ => Err(RuntimeError::new("Set.toList: expected Set")),
            }
        }
        "Set.fromList" => {
            let v = one(args, "Set.fromList")?;
            match v {
                Value::List(items) => Ok(Value::Set(items.into_iter().collect())),
                _ => Err(RuntimeError::new("Set.fromList: expected List")),
            }
        }
        "Set.union" => {
            let (a, b) = two(args, "Set.union")?;
            match (a, b) {
                (Value::Set(set_a), Value::Set(set_b)) => {
                    Ok(Value::Set(set_a.union(&set_b).cloned().collect()))
                }
                _ => Err(RuntimeError::new("Set.union: expected two Sets")),
            }
        }
        "Set.intersect" => {
            let (a, b) = two(args, "Set.intersect")?;
            match (a, b) {
                (Value::Set(set_a), Value::Set(set_b)) => {
                    Ok(Value::Set(set_a.intersection(&set_b).cloned().collect()))
                }
                _ => Err(RuntimeError::new("Set.intersect: expected two Sets")),
            }
        }
        "Set.difference" => {
            let (a, b) = two(args, "Set.difference")?;
            match (a, b) {
                (Value::Set(set_a), Value::Set(set_b)) => {
                    Ok(Value::Set(set_a.difference(&set_b).cloned().collect()))
                }
                _ => Err(RuntimeError::new("Set.difference: expected two Sets")),
            }
        }

        // =====================================================================
        // Env
        // =====================================================================
        "Env.get" => {
            let v = one(args, "Env.get")?;
            match v {
                Value::Str(key) => match std::env::var(&key) {
                    Ok(val) => Ok(some(Value::Str(val))),
                    Err(_) => Ok(none()),
                },
                _ => Err(RuntimeError::new("Env.get: expected String key")),
            }
        }
        "Env.require" => {
            let v = one(args, "Env.require")?;
            match v {
                Value::Str(key) => match std::env::var(&key) {
                    Ok(val) => Ok(ok(Value::Str(val))),
                    Err(_) => Ok(err(Value::Variant {
                        name: "Missing".to_string(),
                        payload: VariantPayload::Record(vec![
                            ("key".to_string(), Value::Str(key)),
                        ]),
                    })),
                },
                _ => Err(RuntimeError::new("Env.require: expected String key")),
            }
        }

        // =====================================================================
        // Json
        // =====================================================================
        "Json.parse" | "JSON.parse" => {
            let v = one(args, "Json.parse")?;
            let text = match v {
                Value::Str(s) => s,
                Value::Bytes(b) => String::from_utf8(b)
                    .map_err(|_| RuntimeError::new("Json.parse: bytes are not valid UTF-8"))?,
                _ => return Err(RuntimeError::new("Json.parse: expected String or Bytes")),
            };
            match serde_json::from_str::<serde_json::Value>(&text) {
                Ok(j) => Ok(ok(json_to_value(j))),
                Err(e) => Ok(err(Value::Variant {
                    name: "InvalidJson".to_string(),
                    payload: VariantPayload::Record(vec![
                        ("offset".to_string(), Value::Int(e.column() as i64)),
                    ]),
                })),
            }
        }
        "Json.serialize" | "JSON.serialize" => {
            let v = one(args, "Json.serialize")?;
            let j = value_to_json(&v);
            let s = serde_json::to_string(&j)
                .map_err(|e| RuntimeError::new(format!("Json.serialize: {}", e)))?;
            Ok(Value::Bytes(s.into_bytes()))
        }

        // =====================================================================
        // Http / HttpServer / Response (trusted stubs — sync model returns Ok stubs)
        // =====================================================================
        "Http.get" | "Http.delete" => {
            Ok(ok(stub_response(200)))
        }
        "Http.post" | "Http.put" | "Http.patch" => {
            Ok(ok(stub_response(200)))
        }
        "HttpServer.start" => {
            Ok(ok(Value::Unit))
        }
        "Response.json" => {
            let (status, body) = two(args, "Response.json")?;
            let status_int = match status {
                Value::Int(n) => n,
                _ => 200,
            };
            let body_bytes = serde_json::to_vec(&value_to_json(&body)).unwrap_or_default();
            Ok(Value::Record(vec![
                ("status".to_string(), Value::Int(status_int)),
                ("body".to_string(), Value::Bytes(body_bytes)),
            ]))
        }
        "Response.err" => {
            let (status, e) = two(args, "Response.err")?;
            let status_int = match status {
                Value::Int(n) => n,
                _ => 500,
            };
            let body_bytes = serde_json::to_vec(&value_to_json(&e)).unwrap_or_default();
            Ok(Value::Record(vec![
                ("status".to_string(), Value::Int(status_int)),
                ("body".to_string(), Value::Bytes(body_bytes)),
            ]))
        }

        // =====================================================================
        // GraphQL (trusted stub)
        // =====================================================================
        "GraphQL.execute" | "GraphQL.query" => {
            Ok(ok(Value::Record(vec![
                ("data".to_string(), Value::Unit),
                ("errors".to_string(), Value::List(vec![])),
            ])))
        }

        // =====================================================================
        // File I/O
        // =====================================================================
        "File.read" => {
            let v = one(args, "File.read")?;
            match v {
                Value::Str(path) => match std::fs::read_to_string(&path) {
                    Ok(contents) => Ok(Value::Str(contents)),
                    Err(e) => Err(RuntimeError::new(format!("File.read: {}", e))),
                },
                _ => Err(RuntimeError::new("File.read: expected String path")),
            }
        }
        "File.readLines" => {
            let v = one(args, "File.readLines")?;
            match v {
                Value::Str(path) => match std::fs::read_to_string(&path) {
                    Ok(contents) => Ok(Value::List(
                        contents
                            .lines()
                            .map(|l| Value::Str(l.to_string()))
                            .collect(),
                    )),
                    Err(e) => Err(RuntimeError::new(format!("File.readLines: {}", e))),
                },
                _ => Err(RuntimeError::new("File.readLines: expected String path")),
            }
        }

        _ => Err(RuntimeError::new(format!("unknown stdlib function '{}'", name))),
    }
}

// =========================================================================
// HTTP stub helper
// =========================================================================

fn stub_response(status: i64) -> Value {
    Value::Record(vec![
        ("status".to_string(), Value::Int(status)),
        ("body".to_string(), Value::Bytes(b"{}".to_vec())),
    ])
}

// =========================================================================
// JSON <-> Value helpers
// =========================================================================

pub fn json_to_value(j: serde_json::Value) -> Value {
    match j {
        serde_json::Value::Null => Value::Unit,
        serde_json::Value::Bool(b) => Value::Bool(b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Int(i)
            } else {
                Value::Float(n.as_f64().unwrap_or(0.0))
            }
        }
        serde_json::Value::String(s) => Value::Str(s),
        serde_json::Value::Array(arr) => {
            Value::List(arr.into_iter().map(json_to_value).collect())
        }
        serde_json::Value::Object(map) => {
            Value::Record(map.into_iter().map(|(k, v)| (k, json_to_value(v))).collect())
        }
    }
}

pub fn value_to_json(v: &Value) -> serde_json::Value {
    match v {
        Value::Unit => serde_json::Value::Null,
        Value::Bool(b) => serde_json::Value::Bool(*b),
        Value::Int(n) => serde_json::json!(*n),
        Value::Float(f) => serde_json::json!(*f),
        Value::Str(s) => serde_json::Value::String(s.clone()),
        Value::Bytes(b) => {
            serde_json::Value::String(String::from_utf8_lossy(b).into_owned())
        }
        Value::List(items) => {
            serde_json::Value::Array(items.iter().map(value_to_json).collect())
        }
        Value::Record(fields) => {
            let obj: serde_json::Map<String, serde_json::Value> =
                fields.iter().map(|(k, v)| (k.clone(), value_to_json(v))).collect();
            serde_json::Value::Object(obj)
        }
        Value::Map(map) => {
            let obj: serde_json::Map<String, serde_json::Value> = map
                .iter()
                .filter_map(|(k, v)| {
                    if let Value::Str(ks) = k {
                        Some((ks.clone(), value_to_json(v)))
                    } else {
                        None
                    }
                })
                .collect();
            serde_json::Value::Object(obj)
        }
        Value::Variant { name, payload } => {
            let mut obj = serde_json::Map::new();
            obj.insert("tag".to_string(), serde_json::Value::String(name.clone()));
            match payload {
                VariantPayload::Unit => {}
                VariantPayload::Tuple(inner) => {
                    obj.insert("value".to_string(), value_to_json(inner));
                }
                VariantPayload::Record(fields) => {
                    for (k, v) in fields {
                        obj.insert(k.clone(), value_to_json(v));
                    }
                }
            }
            serde_json::Value::Object(obj)
        }
        _ => serde_json::Value::Null,
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
