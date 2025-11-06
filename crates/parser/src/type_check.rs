use lexer::{ Token, TokenType };

use crate::{ ASTNode, symbol::{ SymbolTable, VariableInfo }, ParserError};

pub struct TypeChecker<'a> {
    symbol_table: SymbolTable<'a>
}

impl<'a> TypeChecker<'a> {
    pub fn new() -> Self {
        TypeChecker {
            symbol_table: SymbolTable::new()
        }
    }

    pub fn check(&mut self, ast: &Vec<ASTNode<'a>>) -> Result<(), ParserError<'a>> {
        for node in ast {
            self.check_node(node)?;
        }

        Ok(())
    }

    fn check_node(&mut self, node: &ASTNode<'a>) -> Result<(), ParserError<'a>> {
        match node {
            ASTNode::FunctionDeclaration { name: _, parameters, return_type, body } => {
                self.validate_type(&return_type.lexeme, return_type.clone())?;

                self.symbol_table.enter_scope();

                for (_param_name, param_type) in parameters {
                    self.validate_type(&param_type.lexeme, param_type.clone())?;

                    let var_info = VariableInfo {
                        type_name: param_type.lexeme.to_string(),
                        is_mutable: false,
                        _phantom: std::marker::PhantomData
                    };
                    self.symbol_table.define_variable(_param_name.lexeme, var_info, _param_name.clone());
                }

                for stmt in body {
                    self.check_node(stmt)?;
                }

                self.symbol_table.exit_scope();
            }

            ASTNode::VariableDeclaration { is_const, name, type_annotation, initializer } => {
                self.check_node(initializer)?;

                let inferred = if let Some(type_tok) = type_annotation {
                    self.validate_type(&type_tok.lexeme, type_tok.clone())?;
                    self.check_type_compatibility(initializer, &type_tok.lexeme)?;
                    type_tok.lexeme.to_string()
                } else {
                    self.infer_type(initializer)?
                };

                let is_mutable = !is_const;
                let var_info = VariableInfo {
                    type_name: inferred,
                    is_mutable,
                    _phantom: std::marker::PhantomData
                };

                self.symbol_table.define_variable(name.lexeme, var_info, name.clone())?;
            }

            ASTNode::AssignmentExpression { target, operator, value } => {
                let (var_name, var_token) = match **target {
                    ASTNode::VariableExpression { ref name } => (name.lexeme, name.clone()),
                    _ => return Err(ParserError::Generic {
                        message: "assignment target must be a variable".to_string(),
                        token: operator.clone(),
                        help: None
                    })
                };

                let var_info = self.symbol_table.get_variable(var_name)
                    .ok_or_else(|| ParserError::Generic {
                        message: format!("unknown variable: '{}'", var_name),
                        token: var_token.clone(),
                        help: None
                    })?;

                if !var_info.is_mutable {
                    return Err(ParserError::Generic {
                        message: format!(
                            "cannot reassign '{}'. binding is const",
                            var_name
                        ),
                        token: var_token.clone(),
                        help: None
                    });
                }

                self.check_type_compatibility(value, &var_info.type_name)?;
                self.check_node(value)?;
            }

            ASTNode::ReturnStatement { value } => {
                self.check_node(value)?;
            }

            ASTNode::FunctionCallExpression { name: _, arguments } => {
                for arg in arguments {
                    self.check_node(arg)?;
                }
            }

            ASTNode::BinaryExpression { left, operator: _, right } => {
                self.check_node(left)?;
                self.check_node(right)?;
            }

            ASTNode::Expression { token: _ } => {
            }

            ASTNode::VariableExpression { name } => {
                if self.symbol_table.get_variable(name.lexeme).is_none() {
                    return Err(ParserError::Generic {
                        message: format!("unknown variable '{}'", name.lexeme),
                        token: name.clone(),
                        help: None
                    });
                }
            }

            ASTNode::Primtive { token: _ } => {
            }
        }

        Ok(())
    }

    fn infer_type(&self, node: &ASTNode<'a>) -> Result<String, ParserError<'a>> {
        match node {
            ASTNode::Expression { token } => {
                match &token.token_type {
                    TokenType::BoolLiteral(_) => Ok("bool".to_string()),
                    TokenType::CharLiteral(_) => Ok("char".to_string()),
                    TokenType::FloatLiteral(_) => Ok("f64".to_string()),
                    TokenType::IntLiteral(_) => Ok("i32".to_string()),

                    _ => Err(ParserError::Generic {
                        message: format!("cannot infer type from token: {:?}", token.token_type),
                        token: token.clone(),
                        help: Some(format!("consider annotation the variable"))
                    })                
                }
            }

            ASTNode::BinaryExpression { left, operator, right } => {
                let left_type = self.infer_type(left)?;
                let right_type = self.infer_type(right)?;

                if left_type != right_type {
                    return Err(ParserError::Generic {
                        message: format!(
                            "type mismatch in binary expression: '{}' and '{}'",
                            left_type, right_type
                        ),
                        token: operator.clone(),
                        help: None
                    }); 
                }

                Ok(left_type)
            }

            ASTNode::FunctionCallExpression { name, arguments: _ } => {
                // wont necessarily need type inference for function calls
                // in the sense of computing it since the return type is 
                // always specified when declaring a function, its more
                // so looking the return type up
                // 
                // example:
                //      fn add(a: i32, b: i32) -> i32 {
                //          return a + b;
                //      }
                //
                //      fn main() -> void {
                //          const sum = add(1, 2);
                //      }
                //
                // the type of sum is an signed integer value that fits into 32 bits
                // if something like 10 which can be represented with only 8 bits,
                // its just promoted to an i32
                Err(ParserError::Generic {
                    message: format!("cannot infer type of function call '{}'", name.lexeme),
                    token: name.clone(),
                    help: None
                })
            }
            ASTNode::VariableExpression { name } => {
                let var_info = self.symbol_table.get_variable(name.lexeme)
                    .ok_or_else(|| ParserError::Generic {
                        message: format!("unknown variable '{}'", name.lexeme),
                        token: name.clone(),
                        help: None
                    })?;

                Ok(var_info.type_name.clone())
            }

            _ => Err(ParserError::Generic {
                message: "cannot infer type from this expression".to_string(),
                token: Token { 
                    token_type: TokenType::EOF,
                    lexeme: "",
                    line: 1,
                    column: 1
                },
                help: None
            })        
        }
    }

    fn validate_type(&self, type_name: &str, token: Token<'a>) -> Result<(), ParserError<'a>> {
        if self.is_primitive(type_name) {
            Ok(())
        } else {
            // TODO: this will be updated when structs come into play
            Err(ParserError::Generic {
                message: format!("unknown type '{}'", type_name),
                token,
                help: None
            })        
        }
    }

    fn check_type_compatibility(&self, node: &ASTNode<'a>, expected: &str) -> Result<(), ParserError<'a>> {
        match node {
            ASTNode::Expression { token } => {
                match &token.token_type {
                    TokenType::BoolLiteral(_) => {
                        if expected != "bool" {
                            return Err(ParserError::TypeMismatch { 
                                token: token.clone(),
                                expected: expected.to_string(),
                                found: token.clone()
                            });
                        }
                    }

                    TokenType::IntLiteral(value) => {
                        if self.is_integer(expected) {
                            self.check_integer_bounds(*value, expected, token.clone())?;
                        } else if self.is_float(expected) {
                            let inferred = self.infer_type(node)?;
                            // return as error for now
                            // when mature enough we'll implicitly convert int to float
                            // at that stage it should only return an error when the 
                            // annotation is an integer and the value is a float
                            return Err(ParserError::Generic {
                                message: format!(
                                    "mismatched types: expected type '{}', found integer literal",
                                    expected
                                ),
                                token: token.clone(),
                                help: Some(format!(
                                    "consider changing the type annotation to '{}'",
                                    inferred
                                ))
                            });
                        }
                    }

                    TokenType::FloatLiteral(_) => {
                        if self.is_float(expected) {
                            // TODO: float checking bounds
                        } else if self.is_integer(expected) {
                            let inferred = self.infer_type(node)?;

                            return Err(ParserError::Generic {
                                message: format!(
                                    "mismatched types: expected type '{}', found float literal",
                                    expected 
                                ),
                                token: token.clone(),
                                help: Some(format!(
                                    "consider changing the type annotation to '{}'",
                                    inferred
                                ))
                            });
                        } else {
                            return Err(ParserError::TypeMismatch {
                                token: token.clone(),
                                expected: expected.to_string(),
                                found: token.clone()
                            });
                        }
                    }

                    TokenType::CharLiteral(_) => {
                        if expected != "char" {
                            return Err(ParserError::TypeMismatch {
                                token: token.clone(),
                                expected: expected.to_string(),
                                found: token.clone()
                            });
                        }
                    }

                    _ => {}
                }
                Ok(())
            }

            ASTNode::BinaryExpression { left, operator: _, right } => {
                self.check_type_compatibility(left, expected)?;
                self.check_type_compatibility(right, expected)?;

                Ok(())
            }

            _ => Ok(())
        }
    }

    fn is_primitive(&self, type_name: &str) -> bool {
        matches!(type_name,
            "i8" | "i16" | "i32" | "i64" | "isize" |
            "u8" | "u16" | "u32" | "u64" | "usize" |
            "f32" | "f64" |
            "bool" | "char" | "void"
        )
    }

    fn is_integer(&self, type_name: &str) -> bool {
        matches!(type_name,
            "i8" |  "i16" | "i32" | "i64" | "isize" |
            "u8" | "u16" | "u32" | "u64" | "usize"
        )
    }

    fn is_float(&self, type_name: &str) -> bool {
        matches!(type_name,
            "f32" | "f64"
        )
    }

    fn get_type_bounds(&self, type_name: &str) -> (String, String) {
        match type_name {
            "isize" => (isize::MIN.to_string(), isize::MAX.to_string()),
            "i8" => (i8::MIN.to_string(), i8::MAX.to_string()),
            "i16" => (i16::MIN.to_string(), i16::MAX.to_string()),
            "i32" => (i32::MIN.to_string(), i32::MAX.to_string()),
            "i64" => (i64::MIN.to_string(), i64::MAX.to_string()),
            "usize" => ("0".to_string(), usize::MAX.to_string()),
            "u8" => ("0".to_string(), u8::MAX.to_string()),
            "u16" => ("0".to_string(), u16::MAX.to_string()),
            "u32" => ("0".to_string(), u32::MAX.to_string()),
            "u64" => ("0".to_string(), u64::MAX.to_string()),

            _ => ("?".to_string(), "?".to_string())
        }
    }

    fn check_integer_bounds(&self, value: i64, type_name: &str, token: Token<'a>) -> Result<(), ParserError<'a>> {
        let bounds = match type_name {
            "isize" => value >= isize::MIN as i64 && value <= isize::MAX as i64,
            "i8" => value >= i8::MIN as i64 && value <= i8::MAX as i64,
            "i16" => value >= i16::MIN as i64 && value <= i16::MAX as i64,
            "i32" => value >= i32::MIN as i64 && value <= i32::MAX as i64,
            "usize" => value >= 0 && (value as u64) <= usize::MAX as u64,
            "u8" => value >= 0 && value <= u8::MAX as i64,
            "u16" => value >= 0 && value <= u16::MAX as i64,
            "u32" => value >= 0 && value <= u32::MAX as i64,
            "u64" => value >= 0,
            
            _ => return Ok(())
        };

        if !bounds {
            let (min, max) = self.get_type_bounds(type_name);
            Err(ParserError::Generic {
                message: format!(
                    "integer literal '{}' is out of bound for type '{}' (range: {} to {})",
                    value, type_name, min, max
                ),
                token,
                help: None
            })
        } else {
            Ok(())
        }
    }
}
