use std::collections::HashMap;
use errors::error::Span;
use parser::Annotation;
use crate::types::Type;
use crate::Constant;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DefID(pub usize);

impl std::fmt::Display for DefID {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "DefID({})", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeID(pub usize);

#[derive(Debug, Clone)]
pub enum DefKind {

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

    Alias {
        target_path: Vec<String>,
    },

    Constant {
        ty: Type,
        value: Constant,
    },
}

#[derive(Debug, Clone)]
pub struct SymbolInfo {
    pub name: String,
    pub span: Span,
    pub absolute_path: Vec<String>,
    pub kind: DefKind,
}

#[derive(Default)]
pub struct HIRContext {
    pub definitions: HashMap<DefID, SymbolInfo>,
    pub types: HashMap<TypeID, Type>,
    next_def_id: usize,
    next_type_id: usize,
}

impl HIRContext {

    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert_def(&mut self, info: SymbolInfo) -> DefID {
        let id = DefID(self.next_def_id);

        self.next_def_id += 1;
        self.definitions.insert(id, info);

        id
    }

    pub fn get_def(&self, id: DefID) -> Option<&SymbolInfo> {
        self.definitions.get(&id)
    }

    pub fn update_def(&mut self, id: DefID, info: SymbolInfo) {
        self.definitions.insert(id, info);
    }
}
