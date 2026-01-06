use super::CodeGen;

use inkwell::values::BasicValueEnum;

use lexer::{Token, TokenType};
use parser::ast::ASTNode;

impl<'ctx> CodeGen<'ctx> {

    pub fn generate_expression_literal(&self, token: &Token) -> Result<Option<BasicValueEnum<'ctx>>, String> {
        let inferred = match &token.token_type {
            TokenType::IntLiteral(_) => self.context.i32_type().into(),
            TokenType::FloatLiteral(_) => self.context.f32_type().into(),
            TokenType::BoolLiteral(_) => self.context.bool_type().into(),
            TokenType::CharLiteral(_) => self.context.i8_type().into(),
            _ => return Err("unsupported literal type".into())
        };

        Ok(Some(self.generate_literal(token, inferred)))
    }

    pub fn generate_literal(&self, token: &Token, t: inkwell::types::BasicTypeEnum<'ctx>) -> BasicValueEnum<'ctx> {
        match &token.token_type {
            TokenType::IntLiteral(v) => t.into_int_type().const_int((*v as i64) as u64, true).into(),
            TokenType::FloatLiteral(v) => t.into_float_type().const_float(*v).into(),
            TokenType::BoolLiteral(v) => t.into_int_type().const_int(if *v { 1 } else { 0 }, false).into(),
            TokenType::CharLiteral(v) => t.into_int_type().const_int(*v as u64, false).into(),
            _ => panic!("unsupported literal")
        }
    }

    pub fn generate_binary_expression(&mut self, left: &ASTNode, operator: &Token, right: &ASTNode) -> 
                                Result<Option<BasicValueEnum<'ctx>>, String> 
    {
        let left_val = self.generate_node(left)?.unwrap();
        let right_val = self.generate_node(right)?.unwrap();

        match operator.token_type {
            TokenType::Plus => {
                let result = self.builder.build_int_add(
                    left_val.into_int_value(),
                    right_val.into_int_value(),
                    "addtmp"
                );

                Ok(Some(result.into()))
            },

            TokenType::Minus => {
                let result = self.builder.build_int_sub(
                    left_val.into_int_value(), 
                    right_val.into_int_value(),
                    "subtmp"
                );

                Ok(Some(result.into()))
            },

            TokenType::Star => {
                let result = self.builder.build_int_mul(
                    left_val.into_int_value(),
                    right_val.into_int_value(), 
                    "multtemp"
                );

                Ok(Some(result.into()))
            }

            TokenType::ForwardSlash => {
                let result = self.builder.build_int_signed_div(
                    left_val.into_int_value(),
                    right_val.into_int_value(), 
                    "divtmp"
                );

                Ok(Some(result.into()))
            }

            _ => Err(format!("error: unsupported binary operator: {:?}", operator.token_type)),
        }
    }

   pub fn infer_type(&self, token: &Token) -> Result<inkwell::types::BasicTypeEnum<'ctx>, String> {
        Ok(match &token.token_type {
            TokenType::IntLiteral(_) => self.context.i32_type().into(),
            TokenType::FloatLiteral(_) => self.context.f64_type().into(),
            TokenType::BoolLiteral(_) => self.context.bool_type().into(),
            TokenType::CharLiteral(_) => self.context.i8_type().into(),
            _ => return Err("cannot infer type".into())
        })
    } 
}
