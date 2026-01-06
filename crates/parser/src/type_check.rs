use lexer::{ Token, TokenType };

use crate::{ symbol::{ FunctionInfo, SymbolTable, VariableInfo }, ASTNode, ParserError};


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
            if let ASTNode::FunctionDeclaration { name, parameters, return_type, .. } = node {
                self.register_function(name, parameters, return_type)?;
            }
        }

        for node in ast {
            self.check_node(node)?;
        }

        Ok(())
    }

    fn register_function(&mut self, name: &Token<'a>, parameters: &[(Token<'a>, Box<ASTNode<'a>>)],
                        return_type: &Box<ASTNode<'a>>) -> Result<(), ParserError<'a>> 
    {
        let mut param_types = Vec::new();
        for (_, param_t_node) in parameters {
            self.validate_type(param_t_node)?;
            param_types.push(self.get_type_name(param_t_node)?);
        }

        self.validate_type(return_type)?;
        let return_type_name = self.get_type_name(return_type)?;

        let info = FunctionInfo {
            param_types,
            return_type: return_type_name,
            _phantom: std::marker::PhantomData
        };

        self.symbol_table.define_function(name.lexeme, info, name.clone())
    }

    fn check_node(&mut self, node: &ASTNode<'a>) -> Result<(), ParserError<'a>> {
        match node {
            ASTNode::FunctionDeclaration { name: _, parameters, return_type, body } => {
                self.symbol_table.enter_scope();

                for (_param_name, param_type) in parameters {
                    let type_name = self.get_type_name(param_type)?;
                    let var_info = VariableInfo {
                        type_name, 
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

                let inferred = if let Some(type_node) = type_annotation {
                    self.validate_type(type_node)?;
                    let type_name = self.get_type_name(type_node)?;

                    if let (ASTNode::ArrayType { element_type, size, .. }, 
                            ASTNode::ArrayInitializer { elements, token }) = (&**type_node, &**initializer) 
                    {
                        let expected_size = match **size {
                            ASTNode::Expression {
                                token: Token {
                                    token_type: TokenType::IntLiteral(val),
                                    ..
                                }
                            } => val as usize,
                            _ => return Err(ParserError::Generic {
                                message: "array size must be a const integer".to_string(),
                                token: token.clone(),
                                help: None
                            })
                        };

                        if elements.len() != expected_size {
                            return Err(ParserError::Generic {
                                message: format!("expected {} elements in array initializer, found {}", expected_size, elements.len()),
                                token: token.clone(),
                                help: None
                            });
                        }

                        let element_type = self.get_type_name(element_type)?;
                        for elem in elements {
                            self.check_type_compatibility(elem, &element_type)?;
                        }
                    } else {
                        self.check_type_compatibility(initializer, &type_name);
                    }

                    type_name
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

                if let (true, ASTNode::ArrayInitializer { token, .. }) = (var_info.type_name.starts_with('['), &**value) {
                    return Err(ParserError::Generic {
                        message: "cannot assign to array, assignment to array elements not yet supported".to_string(),
                        token: token.clone(),
                        help: Some("array assignment will be supported via indexing".to_string())
                    });
                }

                self.check_type_compatibility(value, &var_info.type_name)?;
                self.check_node(value)?;
            }

            ASTNode::ReturnStatement { value } => {
                self.check_node(value)?;
            }

            ASTNode::FunctionCallExpression { name, arguments } => {
                if name.lexeme == "println" {
                    for arg in arguments {
                        self.check_node(arg)?;
                    }
                    
                    return Ok(());
                }

                let func_info = self.symbol_table.get_function(name.lexeme)
                    .ok_or_else(|| ParserError::Generic {
                        message: format!("call to undefined function '{}'", name.lexeme),
                        token: name.clone(),
                        help: None,
                    })?;

                if arguments.len() != func_info.param_types.len() {
                    return Err(ParserError::Generic {
                        message: format!(
                            "function '{}' expected {} arguments, got {}",
                            name.lexeme,
                            func_info.param_types.len(),
                            arguments.len()
                        ),
                        token: name.clone(),
                        help: None
                    })
                }

                let expected_types: Vec<String> = func_info.param_types.clone();

                for (arg_node, expected) in arguments.iter().zip(expected_types.iter()) {
                    self.check_node(arg_node)?;
                    let inferred = self.infer_type(arg_node)?;

                    if &inferred != expected {
                        let (token, found) = match arg_node {
                            ASTNode::VariableExpression { name } => (name.clone(), name.lexeme),
                            ASTNode::Expression { token } => (token.clone(), token.lexeme),
                            ASTNode::ArrayInitializer { token, .. } => (token.clone(), "{...}"),

                            _ => (name.clone(), "expression")
                        };

                        return Err(ParserError::TypeMismatch {
                            token: token.clone(),
                            expected: expected.to_string(),
                            found: Token {
                                token_type: TokenType::Identifier(inferred.clone()),
                                lexeme: found,
                                ..token
                            }
                        });
                    }
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
                        message: format!("unknown variable '{}'", name.lexeme).to_string(),
                        token: name.clone(),
                        help: None
                    });
                }
            }

            ASTNode::ArrayInitializer { elements, token } => {
                if elements.is_empty() {
                    return Err(ParserError::Generic {
                        message: "cannot infer type of empty array initializer".to_string(),
                        token: token.clone(),
                        help: Some("provide an explicit type annotation\nie. [i32, 5]".to_string())
                    });
                }

                let first_elem_type = self.infer_type(&elements[0])?;

                for elem in elements.iter().skip(1) {
                    let elem_type = self.infer_type(elem)?;
                    if elem_type != first_elem_type {
                        return Err(ParserError::Generic { 
                            message: format!("expected '{}', found '{}'", first_elem_type, elem_type),
                            token: token.clone(),
                            help: Some("all elements must be the same type".to_string())
                        });
                    }
                }
                
                return Ok(())
            }

            ASTNode::Primtive { token: _ } => {
            }

            _ => {}
        }

        Ok(())
    }

    fn get_type_name(&self, t_node: &ASTNode<'a>) -> Result<String, ParserError<'a>> {
        match t_node {
            ASTNode::TypeIdentifier { type_token } => Ok(type_token.lexeme.to_string()),
            ASTNode::ArrayType { element_type, size, .. } => {
                let elem_name = self.get_type_name(element_type)?;
                let size_str = match &**size {
                    ASTNode::Expression { token: Token { token_type: TokenType::IntLiteral(val), .. } } => val.to_string(),
                    _ => "?".to_string()
                };
                Ok(format!("[{}, {}]", elem_name, size_str))
            }
            _ => Err(ParserError::Generic {
                message: "internal error: expected a type node".to_string(),
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
                        help: Some(format!("consider annotating the variable"))
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

    fn validate_type(&self, t_node: &ASTNode<'a>) -> Result<(), ParserError<'a>> {
        match t_node {
            ASTNode::TypeIdentifier { type_token } => {
                if self.is_primitive(type_token.lexeme) {
                    Ok(())
                } else {
                    // TODO: this will be updated when structs come into play
                    Err(ParserError::Generic {
                        message: format!("unknown type '{}'", type_token.lexeme),
                        token: type_token.clone(),
                        help: None
                    })
                }
            }
            ASTNode::ArrayType { element_type, size, token } => {
                self.validate_type(element_type)?;

                match &**size {
                    ASTNode::Expression { token: size_token } => {
                        if let TokenType::IntLiteral(val) = size_token.token_type {
                            if val < 0 {
                                return Err(ParserError::Generic {
                                    message: "array size cannot be negative".to_string(),
                                    token: size_token.clone(),
                                    help: None
                                });
                            }
                        } else {
                            return Err(ParserError::Generic {
                                message: "array size must be an integer literal".to_string(),
                                token: size_token.clone(),
                                help: None
                            });
                        }
                    }
                    _ => {
                        return Err(ParserError::Generic {
                            message: "array size must be a constant expression".to_string(), 
                            token: token.clone(), 
                            help: Some("only integer literals supported for now".to_string())
                        });
                    }
                }
                Ok(())
            }
            _ => Err(ParserError::Generic {
                message: "internal error: invalid node passed to validate_type".to_string(),
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

            ASTNode::ArrayInitializer { token, .. } => {
                // if this is reached, its either a simple {1, 2}
                // or a mismatch like, let x: i32 = {1, 2}
                if !expected.starts_with('[') {
                    return Err(ParserError::TypeMismatch { 
                        token: token.clone(), 
                        expected: expected.to_string(), 
                        found: token.clone()
                    });
                }

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
