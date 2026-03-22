use std::collections::HashMap;
use super::{Type, TypeDef, FnSig, EffectSet};

// =============================================================================
// Type environment — scoped bindings for variables, types, functions, modules
// =============================================================================

#[derive(Debug, Clone)]
pub struct TypeEnv {
    /// Stack of scopes. Each scope maps variable names to their types.
    scopes: Vec<HashMap<String, Type>>,

    /// Registered type definitions: type Name<...> = ...
    pub type_defs: HashMap<String, TypeDef>,

    /// Registered function signatures: fn name { effects In -> Out ... }
    pub fn_sigs: HashMap<String, FnSig>,

    /// Module method signatures: Module.method -> FnSig
    pub module_methods: HashMap<String, HashMap<String, FnSig>>,

    /// Known effect names (built-in + user-defined)
    pub known_effects: HashMap<String, Vec<FnSig>>,
}

impl TypeEnv {
    pub fn new() -> Self {
        let mut env = TypeEnv {
            scopes: vec![HashMap::new()],
            type_defs: HashMap::new(),
            fn_sigs: HashMap::new(),
            module_methods: HashMap::new(),
            known_effects: HashMap::new(),
        };
        env.register_builtins();
        env
    }

    // =========================================================================
    // Scope management
    // =========================================================================

    pub fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    pub fn pop_scope(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
    }

    pub fn bind(&mut self, name: &str, ty: Type) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_string(), ty);
        }
    }

    pub fn lookup(&self, name: &str) -> Option<&Type> {
        for scope in self.scopes.iter().rev() {
            if let Some(ty) = scope.get(name) {
                return Some(ty);
            }
        }
        None
    }

    // =========================================================================
    // Type definitions
    // =========================================================================

    pub fn register_type(&mut self, name: &str, def: TypeDef) {
        self.type_defs.insert(name.to_string(), def);
    }

    pub fn lookup_type(&self, name: &str) -> Option<&TypeDef> {
        self.type_defs.get(name)
    }

    /// Look up a variant name across all registered sum types.
    /// Returns (type_name, variant_def) if found.
    pub fn lookup_variant(&self, variant_name: &str) -> Option<(String, super::VariantDef)> {
        for (type_name, def) in &self.type_defs {
            if let TypeDef::Sum { variants, .. } = def {
                for v in variants {
                    if v.name == variant_name {
                        return Some((type_name.clone(), v.clone()));
                    }
                }
            }
        }
        None
    }

    // =========================================================================
    // Function signatures
    // =========================================================================

    pub fn register_fn(&mut self, name: &str, sig: FnSig) {
        self.fn_sigs.insert(name.to_string(), sig);
    }

    pub fn lookup_fn(&self, name: &str) -> Option<&FnSig> {
        self.fn_sigs.get(name)
    }

    // =========================================================================
    // Module methods
    // =========================================================================

    pub fn register_module(&mut self, module_name: &str, methods: HashMap<String, FnSig>) {
        self.module_methods.insert(module_name.to_string(), methods);
    }

    pub fn lookup_module_method(&self, module_name: &str, method_name: &str) -> Option<&FnSig> {
        self.module_methods.get(module_name)?.get(method_name)
    }

    // =========================================================================
    // Built-in type and effect registration
    // =========================================================================

    fn register_builtins(&mut self) {
        // Built-in effects
        for name in &["Pure", "IO", "Log", "Metric", "Clock"] {
            self.known_effects.insert(name.to_string(), vec![]);
        }

        // Built-in generic types that the checker needs to know about
        // Result<T, E> = Ok(T) | Err(E)
        self.register_type("Result", TypeDef::Sum {
            type_params: vec!["T".to_string(), "E".to_string()],
            variants: vec![
                super::VariantDef {
                    name: "Ok".to_string(),
                    payload: super::VariantPayload::Tuple(Type::TypeVar("T".to_string())),
                },
                super::VariantDef {
                    name: "Err".to_string(),
                    payload: super::VariantPayload::Tuple(Type::TypeVar("E".to_string())),
                },
            ],
        });

        // Maybe<T> = Some(T) | None
        self.register_type("Maybe", TypeDef::Sum {
            type_params: vec!["T".to_string()],
            variants: vec![
                super::VariantDef {
                    name: "Some".to_string(),
                    payload: super::VariantPayload::Tuple(Type::TypeVar("T".to_string())),
                },
                super::VariantDef {
                    name: "None".to_string(),
                    payload: super::VariantPayload::Unit,
                },
            ],
        });

        // Ordering = LessThan | Equal | GreaterThan
        self.register_type("Ordering", TypeDef::Sum {
            type_params: vec![],
            variants: vec![
                super::VariantDef { name: "LessThan".to_string(), payload: super::VariantPayload::Unit },
                super::VariantDef { name: "Equal".to_string(), payload: super::VariantPayload::Unit },
                super::VariantDef { name: "GreaterThan".to_string(), payload: super::VariantPayload::Unit },
            ],
        });

        // Bool = true | false (built-in primitive, but we may need to know)
        // Duration, Timestamp — opaque types
        self.register_type("Duration", TypeDef::Alias {
            type_params: vec![],
            target: Type::Named("Duration".to_string()),
        });
        self.register_type("Timestamp", TypeDef::Alias {
            type_params: vec![],
            target: Type::Named("Timestamp".to_string()),
        });

        // Register built-in module methods for stdlib types
        self.register_stdlib_methods();
    }

    fn register_stdlib_methods(&mut self) {
        let pure = EffectSet::pure_set();

        // Result.ok, Result.err
        let mut result_methods = HashMap::new();
        result_methods.insert("ok".to_string(), FnSig {
            effects: pure.clone(),
            input: Type::TypeVar("T".to_string()),
            output: Type::Generic { name: "Result".to_string(), args: vec![Type::TypeVar("T".to_string()), Type::TypeVar("E".to_string())] },
        });
        result_methods.insert("err".to_string(), FnSig {
            effects: pure.clone(),
            input: Type::TypeVar("E".to_string()),
            output: Type::Generic { name: "Result".to_string(), args: vec![Type::TypeVar("T".to_string()), Type::TypeVar("E".to_string())] },
        });
        self.module_methods.insert("Result".to_string(), result_methods);

        // Maybe.some, Maybe.none
        let mut maybe_methods = HashMap::new();
        maybe_methods.insert("some".to_string(), FnSig {
            effects: pure.clone(),
            input: Type::TypeVar("T".to_string()),
            output: Type::Generic { name: "Maybe".to_string(), args: vec![Type::TypeVar("T".to_string())] },
        });
        maybe_methods.insert("none".to_string(), FnSig {
            effects: pure.clone(),
            input: Type::Unit,
            output: Type::Generic { name: "Maybe".to_string(), args: vec![Type::TypeVar("T".to_string())] },
        });
        self.module_methods.insert("Maybe".to_string(), maybe_methods);

        // String methods
        let mut string_methods = HashMap::new();
        string_methods.insert("trim".to_string(), FnSig { effects: pure.clone(), input: Type::String, output: Type::String });
        string_methods.insert("lowercase".to_string(), FnSig { effects: pure.clone(), input: Type::String, output: Type::String });
        string_methods.insert("uppercase".to_string(), FnSig { effects: pure.clone(), input: Type::String, output: Type::String });
        string_methods.insert("length".to_string(), FnSig { effects: pure.clone(), input: Type::String, output: Type::Int });
        string_methods.insert("contains".to_string(), FnSig { effects: pure.clone(), input: Type::String, output: Type::Bool });
        string_methods.insert("toString".to_string(), FnSig { effects: pure.clone(), input: Type::TypeVar("T".to_string()), output: Type::String });
        self.module_methods.insert("String".to_string(), string_methods);

        // Int methods
        let mut int_methods = HashMap::new();
        int_methods.insert("toString".to_string(), FnSig { effects: pure.clone(), input: Type::Int, output: Type::String });
        int_methods.insert("toFloat".to_string(), FnSig { effects: pure.clone(), input: Type::Int, output: Type::Float });
        int_methods.insert("abs".to_string(), FnSig { effects: pure.clone(), input: Type::Int, output: Type::Int });
        int_methods.insert("min".to_string(), FnSig { effects: pure.clone(), input: Type::Int, output: Type::Int });
        int_methods.insert("max".to_string(), FnSig { effects: pure.clone(), input: Type::Int, output: Type::Int });
        self.module_methods.insert("Int".to_string(), int_methods);

        // List methods
        let mut list_methods = HashMap::new();
        list_methods.insert("head".to_string(), FnSig {
            effects: pure.clone(),
            input: Type::List(Box::new(Type::TypeVar("T".to_string()))),
            output: Type::Generic { name: "Maybe".to_string(), args: vec![Type::TypeVar("T".to_string())] },
        });
        list_methods.insert("tail".to_string(), FnSig {
            effects: pure.clone(),
            input: Type::List(Box::new(Type::TypeVar("T".to_string()))),
            output: Type::List(Box::new(Type::TypeVar("T".to_string()))),
        });
        list_methods.insert("isEmpty".to_string(), FnSig {
            effects: pure.clone(),
            input: Type::List(Box::new(Type::TypeVar("T".to_string()))),
            output: Type::Bool,
        });
        list_methods.insert("length".to_string(), FnSig {
            effects: pure.clone(),
            input: Type::List(Box::new(Type::TypeVar("T".to_string()))),
            output: Type::Int,
        });
        self.module_methods.insert("List".to_string(), list_methods);

        // Channel.new<T>() — returns Channel<T>
        let io_set = EffectSet::from_names(&["IO".to_string()]);
        let mut channel_methods = HashMap::new();
        channel_methods.insert("new".to_string(), FnSig {
            effects: io_set.clone(),
            input: Type::Unit,
            output: Type::Channel(Box::new(Type::TypeVar("T".to_string()))),
        });
        self.module_methods.insert("Channel".to_string(), channel_methods);

        // Bytes methods
        let mut bytes_methods = HashMap::new();
        bytes_methods.insert("empty".to_string(), FnSig { effects: pure.clone(), input: Type::Unit, output: Type::Bytes });
        bytes_methods.insert("fromString".to_string(), FnSig { effects: pure.clone(), input: Type::String, output: Type::Bytes });
        bytes_methods.insert("length".to_string(), FnSig { effects: pure.clone(), input: Type::Bytes, output: Type::Int });
        self.module_methods.insert("Bytes".to_string(), bytes_methods);
    }
}
