use crate::CodeGen;
use ir::stmt::Stmt;

impl<'ctx> CodeGen<'ctx> {
    pub fn compile_stmt(&mut self, stmt: &Stmt) -> Result<(), String> {
        match stmt {
            Stmt::Var { name, ty, init, .. } => {
                let init_val = self.compile_expr(init)?;
                let alloca = self.create_entry_block_alloca(name, ty);
                self.builder.build_store(alloca, init_val);

                self.variables.insert(name.clone(), alloca);

                Ok(())
            },

            Stmt::Assign { name, value } => {
                let ptr = *self.variables.get(name)
                    .ok_or(format!("ICE: variable '{}' not found in codegen scope", name))?;

                let val = self.compile_expr(value)?;

                self.builder.build_store(ptr, val);

                Ok(())
            }

            Stmt::Expr(expr) => {
                self.compile_expr(expr)?;

                Ok(())
            },
            
            // Pending implementation...
            Stmt::Return(_) | Stmt::If { .. } | Stmt::While { .. } | 
            Stmt::Break | Stmt::Continue => {
                Err(format!("statement not yet implemented in codegen: {:?}", stmt))
            },
        }
    }
}
