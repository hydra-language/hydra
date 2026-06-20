use std::collections::HashMap;
use ir::context::{DefID, DefKind, HIRContext};

#[derive(Default)]
pub struct Scope {
    pub symbols: HashMap<String, DefID>,
    pub parent: Option<Box<Scope>>,
    pub module_path: Vec<String>,
}

impl Scope {
    
    pub fn new(module_path: Vec<String>) -> Self {
        Self {
            symbols: HashMap::new(),
            parent: None,
            module_path
        }
    }
    
    pub fn resolve_path(&self, partial_path: &[String], context: &HIRContext) -> Vec<String> {
        if partial_path.is_empty() { return vec![]; }
        let first = &partial_path[0];
        
        let mut current = Some(self);
        while let Some(scope) = current {
            if let Some(&def_id) = scope.symbols.get(first) {
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

    pub fn define(&mut self, name: String, id: DefID) -> Result<(), String> {
        if self.symbols.contains_key(&name) {
            return Err(format!("symbol '{}' is already defined in this scope", name));
        }

        self.symbols.insert(name, id);

        Ok(())
    }

    pub fn resolve(&self, name: &str, context: &HIRContext) -> Option<DefID> {
        if let Some(&id) = self.symbols.get(name) {
            if let Some(info) = context.get_def(id) {
                if matches!(info.kind, DefKind::Alias { .. }) { return None; }
            }

            return Some(id);
        }

        self.parent.as_ref().and_then(|p| p.resolve(name, context))
    }

    pub fn resolve_absolute(&self, path: &[String], context: &HIRContext) -> Option<DefID> {
        if path.is_empty() { return None; }

        self.resolve(&path[0], context)
    }

    pub fn parent(self) -> Option<Scope> {
        self.parent.map(|b| *b)
    }

    pub fn define_or_update(&mut self, name: String, id: DefID) {
        self.symbols.insert(name, id);
    }
}
