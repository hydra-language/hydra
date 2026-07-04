use crate::CodeGen;
use crate::types::compile_type;

use ir::types::Type;
use mir::{MIRFunction, LocalID, BasicBlockID};

use inkwell::types::BasicMetadataTypeEnum;
use inkwell::types::BasicType;

impl<'c> CodeGen<'c> {

    pub fn generate_function_prototype(&self, function: &MIRFunction) {
        // In MIR, locals 1..=arg_count are the parameters
        let mut param_types: Vec<BasicMetadataTypeEnum> = Vec::new();
        for i in 1..=function.arg_count {
            let ty = &function.locals[i].ty;
            param_types.push(compile_type(self.context, &self.target_data, ty).into());
        }

        let return_type = if function.return_type == Type::VOID {
            self.context.void_type().fn_type(&param_types, false)
        } else {
            let basic_return = compile_type(self.context, &self.target_data, &function.return_type);
            basic_return.fn_type(&param_types, false)
        };
        
        self.module.add_function(&function.name, return_type, None);
    }

    pub fn generate_function_body(&mut self, function: &MIRFunction) -> Result<(), String> {
        let func = self.module.get_function(&function.name).unwrap();
        self.current_fn = Some(func);
        self.blocks.clear();
        self.locals.clear();

        // 1. Create LLVM Basic Blocks for every MIR block
        for (i, _) in function.basic_blocks.iter().enumerate() {
            let bb = self.context.append_basic_block(func, &format!("bb{}", i));
            self.blocks.insert(BasicBlockID(i), bb);
        }

        // 2. Allocate ALL locals in the first block
        let entry_bb = self.blocks.get(&BasicBlockID(0)).unwrap();
        self.builder.position_at_end(*entry_bb);

        for (i, local) in function.locals.iter().enumerate() {
            // ONLY allocate if the type is not VOID
            if local.ty != ir::types::Type::VOID {
                let llvm_ty = compile_type(self.context, &self.target_data, &local.ty);
                let alloca = self.builder.build_alloca(llvm_ty, &format!("_{}", i));
                self.locals.insert(LocalID(i), alloca);
            }

            // If this local is a parameter, store the arg value
            if i > 0 && i <= function.arg_count {
                let arg_val = func.get_nth_param((i - 1) as u32).unwrap();
                // Check if we actually allocated it
                if let Some(alloca) = self.locals.get(&LocalID(i)) {
                    self.builder.build_store(*alloca, arg_val);
                }
            }
        }

        // 3. Compile all statements and terminators
        for (i, block) in function.basic_blocks.iter().enumerate() {
            let llvm_bb = self.blocks.get(&BasicBlockID(i)).unwrap();
            self.builder.position_at_end(*llvm_bb);

            for stmt in &block.statements {
                self.compile_stmt(stmt, function)?;
            }
            
            self.compile_terminator(&block.terminator, function)?;
        }

        Ok(())
    }
}
