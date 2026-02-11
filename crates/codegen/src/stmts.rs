use crate::CodeGen;
use inkwell::AddressSpace;
use ir::stmt::{AssignmentTarget, Stmt};

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
                        let ptr = *self.variables.get(name)
                            .ok_or(format!("ICE: variable '{}' not found in codegen")
                        );

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
                    self.builder.build_return(Some(&val));
                } else {
                    self.builder.build_return(None);
                }

                Ok(())
            },
            
            // Pending implementation
            Stmt::If { .. } | Stmt::While { .. } | 
            Stmt::Break | Stmt::Continue => {
                Err(format!("statement not yet implemented in codegen: {:?}", stmt))
            },
        }
    }
}
