use std::collections::HashMap;

use crate::ParserError;
use lexer::Token;

#[derive(Debug, Clone)]
pub struct VariableInfo<'a> {
    pub type_name: String,
    pub is_mutable: bool,

    // associates the lifetime of Token without owning the data
    pub _phantom: std::marker::PhantomData<&'a ()>
}

type Scope<'a> = HashMap<String, VariableInfo<'a>>;

#[derive(Default)]
pub struct SymbolTable<'a> {
    scopes: Vec<Scope<'a>>
}

impl<'a> SymbolTable<'a> {
    
    pub fn new() -> Self {
        let mut table = SymbolTable {
            scopes: Vec::new()
        };

        table.enter_scope(); // start with global scope
        
        table
    }

    pub fn enter_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    pub fn exit_scope(&mut self) {
        self.scopes.pop();
    }

    pub fn define_variable(&mut self, name: &str, info: VariableInfo<'a>, token: Token<'a>) -> Result<(), ParserError<'a>> {
        let current_scope = self.scopes.last_mut().unwrap();
        if current_scope.contains_key(name) {
            return Err(ParserError::Generic {
                message: format!("'{}' is already defined in this scope", name),
                token,
                help: None,
            });
        }
        current_scope.insert(name.to_string(), info);

        Ok(())
    }

    pub fn get_variable(&self, name: &str) -> Option<&VariableInfo<'a>> {
        for scope in self.scopes.iter().rev() {
            if let Some(info) = scope.get(name) {
                return Some(info);
            }
        }

        None
    }
}
