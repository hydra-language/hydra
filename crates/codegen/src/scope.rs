use std::collections::HashMap;
use inkwell::values::PointerValue;

#[derive(Debug)]
pub struct ScopeTable<'ctx> {
    scopes: Vec<HashMap<String, PointerValue<'ctx>>>
}

impl<'ctx> ScopeTable<'ctx> {

    pub fn new() -> Self {
        Self {
            scopes: vec![HashMap::new()]
        }
    }

    pub fn enter_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    pub fn exit_scope(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        } else {
            self.scopes[0].clear()
        }
    }
    
    pub fn insert(&mut self, name: String, value: PointerValue<'ctx>) -> bool {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name, value).is_some()
        } else {
            false
        }
    }

    pub fn lookup(&self, name: &str) -> Option<PointerValue<'ctx>> {
        for scope in self.scopes.iter().rev() {
            if let Some(val) = scope.get(name) {
                return Some(*val);
            }
        }

        None
    }

    pub fn exists_in_this_scope(&self, name: &str) -> bool {
        if let Some(scope) = self.scopes.last() {
            scope.contains_key(name)
        } else {
            false
        }
    }
}
