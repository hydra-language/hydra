use super::CodeGen;

use inkwell::{IntPredicate, values::{BasicValue, BasicValueEnum}};

use lexer::{Token, TokenType};
use parser::ast::ASTNode;

impl<'ctx> CodeGen<'ctx> {

    pub fn generate_expression_literal(&mut self, token: &Token) -> Result<Option<BasicValueEnum<'ctx>>, String> {
        if let TokenType::StringLiteral(s) = &token.token_type {
            let ptr = self.get_global_string_ptr(s);

            return Ok(Some(ptr.as_basic_value_enum()));
        }

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
            TokenType::IntLiteral(v) => t.into_int_type().const_int((*v) as u64, true).into(),
            TokenType::FloatLiteral(v) => t.into_float_type().const_float(*v).into(),
            TokenType::BoolLiteral(v) => t.into_int_type().const_int(if *v { 1 } else { 0 }, false).into(),
            TokenType::CharLiteral(v) => t.into_int_type().const_int(*v as u64, false).into(),
            _ => panic!("unsupported literal")
        }
    }

    pub fn generate_binary_expression(&mut self, left: &ASTNode, operator: &Token, right: &ASTNode) -> 
                                Result<Option<BasicValueEnum<'ctx>>, String> 
    {

        match operator.token_type {
            TokenType::DoubleAmpersand | TokenType::DoublePipe => {
                return self.generate_logical_expression(left, operator, right);
            }
            _ => {}
        }

        let left_val = self.generate_node(left)?.unwrap();
        let right_val = self.generate_node(right)?.unwrap();

        let build_int_comp = |pred: IntPredicate| {
            let val = self.builder.build_int_compare(
                pred, 
                left_val.into_int_value(),
                right_val.into_int_value(),
                "cmptmp"
            );

            Ok(Some(val.into()))
        };

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

            TokenType::Modulo => {
                let result = self.builder.build_int_signed_rem(
                    left_val.into_int_value(),
                    right_val.into_int_value(),
                    "modtmp"
                );

                Ok(Some(result.into()))
            }

            TokenType::DoubleEqual => build_int_comp(IntPredicate::EQ),
            TokenType::ExclamEqual => build_int_comp(IntPredicate::NE),
            TokenType::LeftAngle   => build_int_comp(IntPredicate::SLT), // Signed Less Than
            TokenType::LessEqual   => build_int_comp(IntPredicate::SLE),
            TokenType::RightAngle  => build_int_comp(IntPredicate::SGT),
            TokenType::GreaterEqual=> build_int_comp(IntPredicate::SGE),

            _ => Err(format!("error: unsupported binary operator: {:?}", operator.token_type)),
        }
    }

    pub fn generate_unary_expression(&mut self, operator: &Token, right: &ASTNode)
                            -> Result<Option<BasicValueEnum<'ctx>>, String>
    {
        let right_val = self.generate_node(right)?.unwrap();

        match operator.token_type {
            TokenType::Minus => {
                if right_val.is_int_value() {
                    Ok(Some(self.builder.build_int_neg(right_val.into_int_value(), "negtmp").into()))
                } else if right_val.is_float_value() {
                    Ok(Some(self.builder.build_float_neg(right_val.into_float_value(), "negftmp").into()))
                } else {
                    Err("unary '-' can only apply to numbers".to_string())
                }
            },

            TokenType::ExclamationMark => {
                let bool_val = right_val.into_int_value();
                let true_val = self.context.bool_type().const_int(1, false);

                Ok(Some(self.builder.build_xor(bool_val, true_val, "nottmp").into()))
            },

            _ => Err(format!("unsupported unary operator: {:?}", operator.token_type))
        }
    }

    pub fn generate_postfix_expression(&mut self, operator: &Token, left: &ASTNode)
                    -> Result<Option<BasicValueEnum<'ctx>>, String>
    {
        let var_name = match left {
            ASTNode::VariableExpression { name } => name.lexeme,
            _ => return Err("postfix operator requires a variable".to_string()),
        };

        let var_ptr = self.symbol_table.lookup(var_name)
                .ok_or_else(|| format!("unknown variable: {}", var_name))?;

        let old_val = self.builder.build_load(var_ptr, "old_val").into_int_value();
        let one = old_val.get_type().const_int(1, false);

        let new_val = match operator.token_type {
            TokenType::PlusPlus => self.builder.build_int_add(old_val, one, "inctmp"),
            TokenType::MinusMinus => self.builder.build_int_sub(old_val, one, "dectmp"),

            _ => return Err("unsupported postfix operator".to_string()),
        };

        self.builder.build_store(var_ptr, new_val);

        Ok(Some(old_val.into()))
    }

    fn generate_logical_expression(&mut self, left: &ASTNode, operator: &Token, right: &ASTNode)
                -> Result<Option<BasicValueEnum<'ctx>>, String>
    {
        let parent_fn = self.current_function.ok_or("logical expression cannot be freestanding")?;

        let left_val = self.generate_node(left)?.ok_or("expected lhs value")?.into_int_value();

        let start_bb = self.builder.get_insert_block().unwrap();
        let right_bb = self.context.append_basic_block(parent_fn, "logic_right");
        let merge_bb = self.context.append_basic_block(parent_fn, "logic_merge");

        match operator.token_type {
            TokenType::DoubleAmpersand => {
                // AND: If left is false, jump to merge (return false). Else check right.
                self.builder.build_conditional_branch(left_val, right_bb, merge_bb);
            },
            TokenType::DoublePipe => {
                // OR: If left is true, jump to merge (return true). Else check right.
                self.builder.build_conditional_branch(left_val, merge_bb, right_bb);
            },
            _ => unreachable!(),
        }

        self.builder.position_at_end(right_bb);
        let right_val = self.generate_node(right)?.ok_or("expected rhs value")?.into_int_value();

        let current_right_bb = self.builder.get_insert_block().unwrap();
        self.builder.build_unconditional_branch(merge_bb);

        self.builder.position_at_end(merge_bb);
        let phi = self.builder.build_phi(self.context.bool_type(), "logic_phi");

        match operator.token_type {
            TokenType::DoubleAmpersand => {
                // AND: Short-circuit result is False (from start_bb)
                let false_val = self.context.bool_type().const_int(0, false);
                phi.add_incoming(&[(&false_val, start_bb), (&right_val, current_right_bb)]);
            },
            TokenType::DoublePipe => {
                // OR: Short-circuit result is True (from start_bb)
                let true_val = self.context.bool_type().const_int(1, false);
                phi.add_incoming(&[(&true_val, start_bb), (&right_val, current_right_bb)]);
            },
            _ => unreachable!(),
        }

        Ok(Some(phi.as_basic_value()))
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
