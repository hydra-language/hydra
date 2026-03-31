use std::collections::HashMap;
use ir::types::Type;
use parser::Annotation;

#[derive(Debug, Clone)]
pub enum Symbol {
    Variable {
        ty: Type,
        is_mutable: bool,
    },

    Function {
        params: Vec<Type>,
        annotations: Vec<Annotation>,
        return_type: Type,
        generic_params: Vec<String>,
    },

    Struct {
        fields: Vec<(String, Type, bool)>,
    },
}

#[derive(Default)]
pub struct Scope {
    symbols: HashMap<String, Symbol>,
    parent: Option<Box<Scope>>,
}

impl Scope {
    
    pub fn new() -> Self {
        Self::default()
    }

    pub fn new_child(parent: Scope) -> Self {
        Self {
            symbols: HashMap::new(),
            parent: Some(Box::new(parent)),
        }
    }

    pub fn define(&mut self, name: String, symbol: Symbol) -> Result<(), String> {
        if self.symbols.contains_key(&name) {
            return Err(format!("symbol '{}' is already defined in this scope", name));
        }

        self.symbols.insert(name.clone(), symbol);

        Ok(())
    }

    pub fn resolve(&self, name: &str) -> Option<&Symbol> {
        if let Some(s) = self.symbols.get(name) {
            return Some(s);
        }

        if let Some(parent) = &self.parent {
            return parent.resolve(name);
        }

        None
    }

    // return to parent scope when leaving a child scope
    pub fn parent(self) -> Option<Scope> {
        self.parent.map(|b| *b)
    }

    pub fn define_or_update(&mut self, name: String, symbol: Symbol) {
        self.symbols.insert(name, symbol);
    }
}
