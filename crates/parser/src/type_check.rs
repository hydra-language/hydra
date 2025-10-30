use lexer::TokenType;

use crate::ASTNode;

pub struct TypeChecker<'a> {
    _phantom: std::marker::PhantomData<&'a ()>,
}

impl<'a> TypeChecker<'a> {
    pub fn new() -> Self {
        TypeChecker {
            _phantom: std::marker::PhantomData,
        }
    }

    pub fn check(&mut self, ast: &Vec<ASTNode<'a>>) -> Result<(), String> {
        for node in ast {
            self.check_node(node)?;
        }

        Ok(())
    }

    fn check_node(&mut self, node: &ASTNode<'a>) -> Result<(), String> {
        match node {
            ASTNode::FunctionDeclaration { name: _, parameters, return_type, body } => {
                self.validate_type(&return_type.lexeme)?;

                for (_param_name, param_type) in parameters {
                    self.validate_type(&param_type.lexeme)?;
                }

                for stmt in body {
                    self.check_node(stmt)?;
                }
            }

            ASTNode::VariableDeclaration { is_const: _, name, type_annotation, initializer } => {
                if let Some(type_tok) = type_annotation {
                    self.validate_type(&type_tok.lexeme)?;
                    self.check_type_compatibility(initializer, &type_tok.lexeme)?;
                } else {
                    let _inferred = self.infer_type(initializer);
                }

                self.check_node(initializer)?
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

            ASTNode::VariableExpression { name: _ } => {
            }

            ASTNode::Primtive { token: _ } => {
            }
        }

        Ok(())
    }

    fn infer_type(&self, node: &ASTNode<'a>) -> Result<String, String> {
        match node {
            ASTNode::Expression { token } => {
                match &token.token_type {
                    TokenType::BoolLiteral(_) => Ok("bool".to_string()),
                    TokenType::CharLiteral(_) => Ok("char".to_string()),
                    TokenType::FloatLiteral(_) => Ok("f64".to_string()),
                    TokenType::IntLiteral(_) => Ok("i32".to_string()),

                    _ => Err(format!("error: cannot infer type from token: {:?}", token.token_type))
                }
            }

            ASTNode::BinaryExpression { left, operator: _, right } => {
                let left_type = self.infer_type(left)?;
                let right_type = self.infer_type(right)?;

                if left_type != right_type {
                    return Err(format!(
                        "type mismatch in binary expression: '{}' and '{}'",
                        left_type, right_type
                    ));
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
                Err(format!("cannot infer type of function call '{}'", name.lexeme))
            }
            ASTNode::VariableExpression { name } => {
                Err(format!("cannot infer type of variable '{}'", name.lexeme))
            }

            _ => Err("cannot infer type from this expression".to_string())
        }
    }

    fn validate_type(&self, type_name: &str) -> Result<(), String> {
        if self.is_primitive(type_name) {
            Ok(())
        } else {
            // TODO: this will be updated when structs come into play
            Err(format!("error: unknown type '{}'", type_name))
        }
    }

    // TODO: move return type to Result<(), ParserError>
    fn check_type_compatibility(&self, node: &ASTNode<'a>, expected: &str) -> Result<(), String> {
        match node {
            ASTNode::Expression { token } => {
                match &token.token_type {
                    TokenType::BoolLiteral(_) => {
                        if expected != "bool" {
                            return Err(format!("type mismatch: expected '{}', found 'bool'", expected));
                        }
                    }

                    TokenType::IntLiteral(value) => {
                        if !self.is_integer(expected) {
                            return Err(format!("type mismatch: expected '{}', found integer literal", expected))
                        }

                        if let Err(e) = self.check_integer_bounds(*value, expected) {
                            return Err(e)?;
                        }
                    }

                    TokenType::FloatLiteral(_) => {
                        if expected != "f32" && expected != "f64" {
                            return Err(format!(
                                "type mismatch: expected '{}', found float literal",
                                expected
                            ));
                        }
                    }

                    TokenType::CharLiteral(_) => {
                        if expected != "char" {
                            return Err(format!("type mismatch: expected '{}', found 'char'", expected));
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

    fn check_integer_bounds(&self, value: i64, type_name: &str) -> Result<(), String> {
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
            Err(format!(
                "integer literal '{}' is out of bounds for type '{}' (range: {}-{})",
                value, type_name, min, max
            ))
        } else {
            Ok(())
        }
    }
}
