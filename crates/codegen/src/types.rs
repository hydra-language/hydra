use super::CodeGen;

use inkwell::types::{BasicTypeEnum, BasicType};

use parser::ast::ASTNode;
use lexer::TokenType;

impl<'ctx> CodeGen<'ctx> {

    fn get_type(&self, type_str: &str) -> BasicTypeEnum<'ctx> {
        match type_str {
            "isize" | "usize" => self.context.ptr_sized_int_type(&self.target_data, None).into(),
            "i8" | "u8" | "char" => self.context.i8_type().into(),
            "i16" | "u16" => self.context.i16_type().into(),
            "i32" | "u32" => self.context.i32_type().into(),
            "i64" | "u64"=> self.context.i64_type().into(),
            "f32" => self.context.f32_type().into(),
            "f64" => self.context.f64_type().into(),
            "bool" => self.context.bool_type().into(),
            _ => panic!("error: unsupported type {}", type_str),
        }
    }

    pub fn get_type_name(&self, type_node: &ASTNode) -> Result<String, String> {
        match type_node {
            ASTNode::TypeIdentifier { type_token } => Ok(type_token.lexeme.to_string()),
            ASTNode::ArrayType { element_type, size, .. } => {
                let elem_name = self.get_type_name(element_type)?;
                let size_str = match &**size {
                    ASTNode::Expression { token } => {
                        if let TokenType::IntLiteral(val) = token.token_type {
                            val.to_string()
                        } else {
                            "?".to_string()
                        }
                    }
                    _ => "?".to_string()
                };
                Ok(format!("[{}, {}]", elem_name, size_str))
            }
            _ => Err("error: expected a type node".to_string())
        }  
    }

    pub fn get_type_from_node(&self, type_node: &ASTNode) -> Result<BasicTypeEnum<'ctx>, String> {
        match type_node {
            ASTNode::TypeIdentifier { type_token } => {
                Ok(self.get_type(type_token.lexeme))
            }
            ASTNode::ArrayType { element_type, size, .. } => {
                let elem_type = self.get_type_from_node(element_type)?;

                // Extract size from the size expression
                let size_value = match &**size {
                    ASTNode::Expression { token } => {
                        match &token.token_type {
                            TokenType::IntLiteral(n) => {
                                if *n < 0 {
                                    return Err("Array size must be non-negative".to_string());
                                }
                                *n as u32
                            }
                            _ => return Err("Array size must be an integer literal".to_string())
                        }
                    }
                    _ => return Err("Array size must be a constant expression".to_string())
                };

                Ok(BasicTypeEnum::ArrayType(elem_type.array_type(size_value)))
            }
            _ => Err("Invalid type annotation".to_string())
        }
    }

}
