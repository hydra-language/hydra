use std::collections::HashMap;
use ir::context::{DefID, DefKind, HIRContext};
use parser::ast::NodeID;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Namespace {
    Type,   // structs, traits, generic params, aliases
    Value,  // variables, functions, constants
}

#[derive(Default)]
pub struct Scope {
    pub symbols: HashMap<(Namespace, String), DefID>,
    pub parent: Option<Box<Scope>>,
    pub module_path: Vec<String>,
}

/// this struct holds the results of the name resolution pass.
/// later phases (like semantic analysis) will use this to look up what a syntax node actually points to.
#[derive(Default)]
pub struct NameResolver {
    // maps an AST NodeID (e.g., a Type::Path or Expr::Variable) to its resolved DefID
    pub resolved_paths: HashMap<NodeID, DefID>,

    // (function declaration, parameter index) -> parameter DefID
    pub parameter_defs: HashMap<(NodeID, usize), DefID>,
}

impl NameResolver {

    pub fn new() -> Self {
        Self {
            resolved_paths: HashMap::new(),
            parameter_defs: HashMap::new(),
        }
    }

    /// records that a specific syntax node points to a specific IR definition
    pub fn record_resolution(&mut self, usage_id: NodeID, definition_id: DefID) {
        self.resolved_paths.insert(usage_id, definition_id);
    }

    /// fetches the resolved definition for a syntax node
    pub fn get_resolution(&self, usage_id: NodeID) -> Option<DefID> {
        self.resolved_paths.get(&usage_id).copied()
    }

    pub fn record_parameter(&mut self, function_id: NodeID, parameter_index: usize, def_id: DefID) {
        self.parameter_defs.insert((function_id, parameter_index), def_id);
    }

    pub fn get_parameter(&self, function_id: NodeID, parameter_index: usize) -> Option<DefID> {
        self.parameter_defs.get(&(function_id, parameter_index)).copied()
    }
}

impl Scope {
    
    pub fn new(module_path: Vec<String>) -> Self {
        Self {
            symbols: HashMap::new(),
            parent: None,
            module_path
        }
    }

    pub fn define(&mut self, namespace: Namespace, name: String, id: DefID) -> Result<(), String> {
        let key = (namespace, name.clone());

        if self.symbols.contains_key(&key) {
            return Err(format!("symbol '{}' is already defined in this namespace", name));
        }

        self.symbols.insert(key, id);

        Ok(())
    }

    pub fn define_or_update(&mut self, namespace: Namespace, name: String, id: DefID) {
        self.symbols.insert((namespace, name), id);
    }

    pub fn resolve(&self, namespace: Namespace, name: &str, context: &HIRContext) -> Option<DefID> {
        let key = (namespace, name.to_string());
        
        if let Some(&id) = self.symbols.get(&key) {
            if let Some(info) = context.get_def(id) {
                if matches!(info.kind, DefKind::Alias { .. }) { return None; }
            }
            return Some(id);
        }

        self.parent.as_ref().and_then(|p| p.resolve(namespace, name, context))
    }

    pub fn resolve_absolute(&self, namespace: Namespace, path: &[String], context: &HIRContext) -> Option<DefID> {
        if path.is_empty() { return None; }
        self.resolve(namespace, &path[0], context)
    }

    pub fn resolve_path(&self, partial_path: &[String], context: &HIRContext) -> Vec<String> {
        if partial_path.is_empty() { return vec![]; }
        let first = &partial_path[0];
        
        let mut current = Some(self);
        while let Some(scope) = current {
            // module names conceptually live in the Type namespace for pathing purposes
            let key = (Namespace::Type, first.to_string());
            
            if let Some(&def_id) = scope.symbols.get(&key) {
                if let Some(info) = context.get_def(def_id) {
                    match &info.kind {
                        DefKind::Alias { target_path } => {
                            let mut resolved = target_path.clone();
                            resolved.extend_from_slice(&partial_path[1..]);
                            return resolved;
                        },
                        _ => {
                            let mut resolved = info.absolute_path.clone();
                            resolved.extend_from_slice(&partial_path[1..]);
                            return resolved;
                        }
                    }
                }
            }
            current = scope.parent.as_deref();
        }

        if first == "std" || first == "core" || first == "alloc" {
            return partial_path.to_vec();
        }

        let mut full = self.module_path.clone();
        full.extend_from_slice(partial_path);
        full
    }

    pub fn parent(self) -> Option<Scope> {
        self.parent.map(|b| *b)
    }
}
