use super::CodeGen;

use inkwell::{IntPredicate, values::{BasicValue, BasicValueEnum, IntValue, PointerMathValue, PointerValue}};

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
        let var_ptr = self.generate_lvalue(target)?;

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

    pub fn generate_lvalue(&mut self, node: &ASTNode) -> Result<PointerValue<'ctx>, String> {
        match node {
            ASTNode::VariableExpression { name } => {
                self.symbol_table.lookup(name.lexeme)
                    .ok_or_else(|| format!("unknown variable: {}", name.lexeme))
            },

            ASTNode::ArrayAccess { array, index, .. } => {
                let array_val = self.generate_node(array)?.unwrap();

                if !array_val.is_pointer_value() {
                    return Err("array access can only be performed on arrays".to_string());
                }
                
                let array_ptr = array_val.into_pointer_value();
                let index_val = self.generate_node(index)?.unwrap().into_int_value();
                
                // Perform runtime bounds check
                let ptr_type = array_ptr.get_type().get_element_type();
                if ptr_type.is_array_type() {
                    let array_len = ptr_type.into_array_type().len();
                    self.generate_bounds_check(index_val, array_len)?;
                }
                
                let i32_type = self.context.i32_type();
                let zero = i32_type.const_int(0, false);
                
                unsafe {
                    Ok(self.builder.build_in_bounds_gep(array_ptr, &[zero, index_val], "elem_ptr"))
                }
            },

            _ => Err("expression is not assignable".to_string())
        }
    }

    fn generate_bounds_check(&mut self, index: IntValue<'ctx>, size: u32) -> Result<(), String> {
        let parent_fn = self.current_function.unwrap();
        let size_val = index.get_type().const_int(size as u64, false);
        
        // Check 0 <= index < size (ULT handles negative check implicitly)
        let in_bounds = self.builder.build_int_compare(IntPredicate::ULT, index, size_val, "bounds_check");
        
        let ok_bb = self.context.append_basic_block(parent_fn, "bounds_ok");
        let err_bb = self.context.append_basic_block(parent_fn, "bounds_err");
        
        self.builder.build_conditional_branch(in_bounds, ok_bb, err_bb);
        
        // --- Error Block ---
        self.builder.position_at_end(err_bb);
        self.call_printf("panic: array index is out of bounds\n", &[]);
        
        // Call exit(1)
        let exit_fn = self.module.get_function("exit").unwrap_or_else(|| {
             let ft = self.context.void_type().fn_type(&[self.context.i32_type().into()], false);

             self.module.add_function("exit", ft, None)
        });

        self.builder.build_call(exit_fn, &[self.context.i32_type().const_int(1, false).into()], "");
        self.builder.build_unreachable();
        
        // --- Ok Block ---
        self.builder.position_at_end(ok_bb);
        
        Ok(())
    }
}
