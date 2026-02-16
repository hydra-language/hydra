use crate::CodeGen;
use inkwell::AddressSpace;
use ir::{stmt::{AssignmentTarget, Stmt}, types::Type};

impl<'c> CodeGen<'c> {

    pub fn compile_stmt(&mut self, stmt: &Stmt) -> Result<(), String> {
        match stmt {
            Stmt::Var { name, ty, init, is_mutable } => {
                let init_val = self.compile_expr(init)?;

                if !is_mutable && self.is_val_const(&init_val) {
                    let global = self.module.add_global(
                        init_val.get_type(), 
                        Some(AddressSpace::default()), 
                        name
                    );

                    global.set_initializer(&init_val);
                    global.set_constant(true);

                    self.variables.insert(name.clone(), global.as_pointer_value());

                    return Ok(());
                }

                let alloca = self.create_entry_block_alloca(name, ty);
                self.builder.build_store(alloca, init_val);

                self.variables.insert(name.clone(), alloca);

                Ok(())
            },

            Stmt::Assign { target, value } => {
                let val = self.compile_expr(value)?;

                match target {
                    AssignmentTarget::Variable(name) => {
                        let ptr =* self.variables.get(name)
                            .ok_or(format!("ICE: variable '{}' not found in codegen", name)
                        )?;

                        self.builder.build_store(ptr, val);
                    },

                    AssignmentTarget::ArrayAccess { array, index } => {
                        let array_val = self.compile_expr(array)?;
                        let array_ptr = array_val.into_pointer_value();

                        let index_val = self.compile_expr(index)?;
                        let index_int = index_val.into_int_value();

                        let element_ptr = unsafe {
                            match array.ty {
                                Type::ARRAY(_, _) => {
                                    let zero = self.context.i64_type().const_int(0, false);
                                    self.builder.build_gep(array_ptr, &[zero, index_int], "elem_ptr")
                                },

                                Type::INFERRED_ARRAY(_) | Type::POINTER(_) => {
                                    self.builder.build_gep(array_ptr, &[index_int], "elem_ptr")
                                },

                                _ => return Err(format!("ICE: assign to non-array type {:?}", array.ty))
                            }
                        };

                        self.builder.build_store(element_ptr, val);
                    },

                    AssignmentTarget::MemberAccess { object, index, .. } => {
                        let obj_val = self.compile_expr(object)?;
                        
                        let struct_ptr = obj_val.into_pointer_value();

                        let field_ptr = self.builder.build_struct_gep(struct_ptr, *index, "field_ptr")
                                .map_err(|_| "llvm gep failed: index out of bounds".to_string())?;

                        self.builder.build_store(field_ptr, val);
                    }
                }

                Ok(())
            }

            Stmt::Expr(expr) => {
                self.compile_expr(expr)?;

                Ok(())
            },

            Stmt::Return(value) => {
                if let Some(expr) = value {
                    let val = self.compile_expr(expr)?;

                    let ret_val = if let Type::STRUCT(_) | Type::ARRAY(_, _) = expr.ty {
                        if val.is_pointer_value() {
                            self.builder.build_load(val.into_pointer_value(), "agg_load")
                        } else {
                            val
                        }
                    } else {
                        val
                    };

                    self.builder.build_return(Some(&ret_val));
                } else {
                    self.builder.build_return(None);
                }

                Ok(())
            },

            Stmt::If { cond, then_block, else_block } => {
                let parent_func = self.current_fn.unwrap();

                let then_bb = self.context.append_basic_block(parent_func, "if_then");
                let else_bb = self.context.append_basic_block(parent_func, "if_else");
                let merge_bb = self.context.append_basic_block(parent_func, "if_merge");

                let cond_val = self.compile_expr(cond)?.into_int_value();

                if else_block.is_some() {
                    self.builder.build_conditional_branch(cond_val, then_bb, else_bb);
                } else {
                    self.builder.build_conditional_branch(cond_val, then_bb, merge_bb);
                }

                self.builder.position_at_end(then_bb);
                for stmt in &then_block.stmts {
                    self.compile_stmt(stmt)?;
                }

                if self.builder.get_insert_block().unwrap().get_terminator().is_none() {
                    self.builder.build_unconditional_branch(merge_bb);
                }

                // 4. Compile 'Else' Block
                self.builder.position_at_end(else_bb);
                if let Some(block) = else_block {
                    for stmt in &block.stmts {
                        self.compile_stmt(stmt)?;
                    }
                    if self.builder.get_insert_block().unwrap().get_terminator().is_none() {
                        self.builder.build_unconditional_branch(merge_bb);
                    }
                } else {
                    // Empty else block just jumps to merge
                    self.builder.build_unconditional_branch(merge_bb);
                }

                // 5. Continue at Merge Block
                self.builder.position_at_end(merge_bb);

                Ok(())
            }

            Stmt::While { cond, body, kind: _} => {
                self.compile_while(cond, body)
            },

            Stmt::Block(block) => {
                for s in &block.stmts {
                    self.compile_stmt(s)?;
                }
                Ok(())
            },

            Stmt::Break => {
                let (_, break_bb) = self.loop_stack.last()
                    .ok_or("break statement used outside of loop")?;

                self.builder.build_unconditional_branch(*break_bb);

                Ok(())
            },

            Stmt::Continue => {
                let (continue_bb, _) = self.loop_stack.last()
                    .ok_or("continue statement used outside of loop")?;

                self.builder.build_unconditional_branch(*continue_bb);

                Ok(())
            }
        }
    }
}
