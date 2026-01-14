use super::CodeGen;

use inkwell::values::{BasicValue, BasicValueEnum};

use lexer::{Token, TokenType};
use parser::ast::ASTNode;


impl<'ctx> CodeGen<'ctx> {

    pub fn generate_variable_declaration(&mut self, name: &Token, type_annotation: &Option<Box<ASTNode>>, 
                                    initializer: &ASTNode) -> Result<Option<BasicValueEnum<'ctx>>, String> 
    {
        let var_name = name.lexeme;

        if self.symbol_table.exists_in_this_scope(var_name) {
            return Err(format!("variable '{}' is already declared in this scope", var_name));
        }

        // Determine the variable type
        let var_type = if let Some(type_node) = type_annotation {
            self.get_type_from_node(type_node)?
        } else {
            // Type inference - arrays must have explicit types
            match initializer {
                ASTNode::ArrayInitializer { .. } => {
                    return Err("error: array variables must have explicit type annotation [type, size]".to_string());
                }
                ASTNode::Expression { token } => {
                    match &token.token_type {
                        TokenType::IntLiteral(_) => self.context.i32_type().into(),
                        TokenType::FloatLiteral(_) => self.context.f64_type().into(),
                        TokenType::BoolLiteral(_) => self.context.bool_type().into(),
                        TokenType::CharLiteral(_) => self.context.i8_type().into(),
                        _ => return Err("Cannot infer type from initializer".to_string())
                    }
                }
                _ => {
                    let init_val = self.generate_node(initializer)?.unwrap();
                    init_val.get_type()
                }
            }
        };

        // Generate initial value
        let initial_value = match initializer {
            ASTNode::Expression { token } => self.generate_literal(token, var_type),
            _ => self.generate_node(initializer)?.unwrap(),
        };


        // Allocate and store
        let alloca = self.create_entry_block_alloca(var_name, initial_value.get_type());
        self.builder.build_store(alloca, initial_value);

        self.symbol_table.insert(var_name.to_string(), alloca);

        Ok(None)
    }

    pub fn generate_variable_load(&mut self, name: &Token) -> Result<Option<BasicValueEnum<'ctx>>, String> {
        let var_name = name.lexeme;

        match self.symbol_table.lookup(var_name) {
            Some(var_ptr) => {
                let ptr_type = var_ptr.get_type().get_element_type();

                if ptr_type.is_array_type() {
                    return Ok(Some(var_ptr.as_basic_value_enum()));
                }

                let loaded = self.builder.build_load(var_ptr, var_name);

                Ok(Some(loaded))
            }

            None => Err(format!("unknown variable: {}", var_name)),
        }
    }

    pub fn generate_assignment(&mut self, target: &ASTNode, operator: &Token, value: &ASTNode) -> 
                        Result<Option<BasicValueEnum<'ctx>>, String> 
    {
        let var_name = match target {
            ASTNode::VariableExpression { name } => name.lexeme,
            _ => return Err("error: assignment target must be a variable".to_string())
        };

        let var_ptr = self.symbol_table.lookup(var_name)
            .ok_or_else(|| format!("Unknown variable in assignment: {}", var_name))?;

        let rhs_val = self.generate_node(value)?.unwrap();

        match operator.token_type {
            TokenType::Equal => {
                self.builder.build_store(var_ptr, rhs_val);

                Ok(None)
            },

            TokenType::PlusEqual | TokenType::MinusEqual | TokenType::StarEqual | 
            TokenType::ForwardSlashEqual | TokenType::ModuloEqual => {
                let current_val = self.builder.build_load(var_ptr, "loadtmp").into_int_value();
                let rhs_int = rhs_val.into_int_value();

                let new_val = match operator.token_type {
                    TokenType::PlusEqual => self.builder.build_int_add(current_val, rhs_int, "addtmp"),
                    TokenType::MinusEqual => self.builder.build_int_sub(current_val, rhs_int, "subtmp"),
                    TokenType::StarEqual => self.builder.build_int_mul(current_val, rhs_int, "multmp"),
                    TokenType::ForwardSlashEqual => self.builder.build_int_signed_div(current_val, rhs_int, "divtmp"),
                    TokenType::ModuloEqual => self.builder.build_int_signed_rem(current_val, rhs_int, "modtmp"),

                    _ => unreachable!(),
                };

                self.builder.build_store(var_ptr, new_val);

                Ok(None)
            }

            _ => Err(format!("error: unsupported assignment operator: {:?}", operator.token_type)),
        }
    }
}
