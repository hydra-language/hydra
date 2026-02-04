use crate::CodeGen;
use ir::{Function, types::Type};
use crate::types::compile_type;
use inkwell::types::BasicType;

impl<'c> CodeGen<'c> {

    pub fn generate_function_prototype(&self, function: &Function) {
        let return_type = if function.return_type == Type::VOID {
            self.context.void_type().fn_type(&[], false)
        } else {
            let basic_return = compile_type(self.context, &self.target_data, &function.return_type);
            basic_return.fn_type(&[], false)
        };
        
        // Note: We aren't adding params to the prototype yet for simplicity
        // You can add them here by mapping func.params -> BasicMetadataTypeEnum
        
        self.module.add_function(&function.name, return_type, None);
    }

    pub fn generate_function_body(&mut self, function: &Function) -> Result<(), String> {
        let func = self.module.get_function(&function.name).unwrap();
        let entry = self.context.append_basic_block(func, "entry");

        self.builder.position_at_end(entry);
        self.current_fn = Some(func);

        // Clear variables from previous function
        self.variables.clear();

        // Compile statements
        for stmt in &function.body.stmts {
            self.compile_stmt(stmt)?;
        }

        let current_block = self.builder.get_insert_block().unwrap();
        if current_block.get_terminator().is_none() {
            if function.return_type == Type::VOID {
                self.builder.build_return(None);
            } else {
                self.builder.build_unreachable();
            }
        }

        Ok(())
    }
}
