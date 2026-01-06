use std::collections::HashMap;

use crate::ParserError;
use lexer::Token;

#[derive(Debug, Clone)]
pub struct FunctionInfo<'a> {
    pub param_types: Vec<String>,
    pub return_type: String,
    pub _phantom: std::marker::PhantomData<&'a ()>
}

#[derive(Debug, Clone)]
pub struct VariableInfo<'a> {
    pub type_name: String,
    pub is_mutable: bool,

    // associates the lifetime of Token without owning the data
    pub _phantom: std::marker::PhantomData<&'a ()>
}

type VarScope<'a> = HashMap<String, VariableInfo<'a>>;
type FuncScope<'a> = HashMap<String, FunctionInfo<'a>>;

#[derive(Default)]
pub struct SymbolTable<'a> {
    var_scopes: Vec<VarScope<'a>>,
    func_scopes: Vec<FuncScope<'a>>
}

impl<'a> SymbolTable<'a> {
    
    pub fn new() -> Self {
        let mut table = SymbolTable {
            var_scopes: Vec::new(),
            func_scopes: Vec::new()
        };

        table.enter_scope(); // start with global scope
        
        table
    }

    pub fn enter_scope(&mut self) {
        self.var_scopes.push(HashMap::new());
        self.func_scopes.push(HashMap::new());
    }

    pub fn exit_scope(&mut self) {
        self.var_scopes.pop();
        self.func_scopes.pop();
    }

    pub fn define_variable(&mut self, name: &str, info: VariableInfo<'a>, token: Token<'a>) -> Result<(), ParserError<'a>> {
        let current_scope = self.var_scopes.last_mut().unwrap();
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
        for scope in self.var_scopes.iter().rev() {
            if let Some(info) = scope.get(name) {
                return Some(info);
            }
        }

        None
    }

    pub fn define_function(&mut self, name: &str, info: FunctionInfo<'a>, token: Token<'a>)
                        -> Result<(), ParserError<'a>> 
    {
        let current_scope = self.func_scopes.last_mut().unwrap();
        if current_scope.contains_key(name) {
            return Err(ParserError::Generic {
                message: format!("function '{}' is already defined", name),
                token,
                help: None
            });
        }
        current_scope.insert(name.to_string(), info);

        Ok(())
    }

    pub fn get_function(&self, name: &str) ->  Option<&FunctionInfo<'a>> {
        for scope in self.func_scopes.iter().rev() {
            if let Some(info) = scope.get(name) {
                return Some(info);
            }
        }

        None
    }
}
