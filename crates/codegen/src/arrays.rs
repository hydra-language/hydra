use super::CodeGen;

use inkwell::values::{BasicValueEnum, PointerValue};
use inkwell::types::{BasicType};

use parser::ast::ASTNode;

impl<'ctx> CodeGen<'ctx> {

    pub fn generate_array_initializer(&mut self, elements: &[ASTNode]) ->
                            Result<Option<BasicValueEnum<'ctx>>, String>
    {
        if elements.is_empty() {
            return Err("array initializer cannot be empty".to_string());
        }

        // 
        let first_val = self.generate_node(&elements[0])?.unwrap();
        let element_type = first_val.get_type();

        let arr_len = elements.len() as u32;
        let arr_type = element_type.array_type(arr_len);
        let arr_ptr = self.builder.build_alloca(arr_type, "array_stack_ptr");

        self.store_element(arr_ptr, 0, first_val)?;

        for (index, elem_node) in elements.iter().enumerate().skip(1) {
            let val = self.generate_node(elem_node)?.unwrap();

            if val.get_type() != element_type {
                return Err(format!("array element at index {} has a mismatched type", index));
            }

            self.store_element(arr_ptr, index as u64, val)?;
        }

        Ok(Some(arr_ptr.into()))
    }

    fn store_element(&self, ptr: PointerValue<'ctx>, index: u64, val: BasicValueEnum<'ctx>) ->
                    Result<(), String>
    {
        let i32_type = self.context.i32_type();
        let zero = i32_type.const_int(0, false);
        let idx = i32_type.const_int(index, false);

        unsafe {
            let elem_ptr = self.builder.build_in_bounds_gep(
                ptr,
                &[zero, idx],
                &format!("elem_{}_ptr", index)
            );

            self.builder.build_store(elem_ptr, val);
        }

        Ok(())
    }
}
