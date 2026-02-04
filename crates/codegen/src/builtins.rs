use inkwell::AddressSpace;
use inkwell::values::{BasicMetadataValueEnum, BasicValueEnum, BasicValue, FunctionValue, PointerValue};
use inkwell::types::BasicTypeEnum;

use ir::expr::{Expr, ExprKind};
use crate::CodeGen;

impl<'c> CodeGen<'c> {

    pub fn get_printf_declaration(&self) -> FunctionValue<'c> {
        if let Some(function) = self.module.get_function("printf") {
            return function;
        }

        let i32_type = self.context.i32_type();
        let i8_ptr_type = self.context.i8_type().ptr_type(AddressSpace::default());
        let printf_type = i32_type.fn_type(&[i8_ptr_type.into()], true);

        self.module.add_function("printf", printf_type, None)
    }

    pub fn compile_println(&mut self, args: &[Expr]) -> Result<BasicValueEnum<'c>, String> 
    {
        let _ = self.get_printf_declaration();

        let fmt_expr = args.first().ok_or("println requires a fmt string")?;
        
        let fmt_literal = match &fmt_expr.kind {
            ExprKind::STRING_LITERAL(s) => s,
            _ => return Err("first arg to println must be a string literal".to_string())
        };

        let mut arg_iter = args.iter().skip(1);
        let parts: Vec<&str> = fmt_literal.split("{}").collect();

        for (i, part) in parts.iter().enumerate() {
            if !part.is_empty() {
                self.call_printf(part, &[]);
            }

            if i < parts.len() - 1 {
                let arg_expr = arg_iter.next()
                    .ok_or("too few arguments for fmt string")?;

                let val = self.compile_expr(arg_expr)?;

                let mut is_array = false;

                if val.is_pointer_value() {
                    let ptr = val.into_pointer_value();
                    let ptr_type = ptr.get_type().get_element_type();

                    if ptr_type.is_array_type() {
                        let arr_type = ptr_type.into_array_type();
                        let len = arr_type.len();

                        self.call_printf("[", &[]);

                        for i in 0..len {
                            if i > 0 {
                                self.call_printf(", ", &[]);
                            }

                            let i32_type = self.context.i32_type();
                            let zero = i32_type.const_int(0, false); 
                            let index = i32_type.const_int(i as u64, false);

                            let elem_ptr = unsafe {
                                self.builder.build_in_bounds_gep(
                                    ptr,
                                    &[zero, index],
                                    "elem_ptr"
                                )
                            };

                            let elem_val = self.builder.build_load(elem_ptr, "elem_val");
                            self.compile_print_value(elem_val)?;
                        }

                        self.call_printf("]", &[]);
                        is_array = true;
                    }
                }

                if !is_array {
                    self.compile_print_value(val)?;
                }
            }
        }
        
        self.call_printf("\n", &[]);

        Ok(self.context.i32_type().const_zero().into())
    }

    fn compile_print_value(&mut self, value: BasicValueEnum<'c>) -> Result<(), String> {
        match value.get_type() {
            BasicTypeEnum::IntType(int) => {
                match int.get_bit_width() {
                    1 => {
                        let bool_val = value.into_int_value();
                        let true_str = self.get_global_string_ptr("true");
                        let false_str = self.get_global_string_ptr("false");

                        let str_val = self.builder.build_select(
                            bool_val, 
                            true_str.as_basic_value_enum(), 
                            false_str.as_basic_value_enum(),
                            "bool_str"
                        );

                        self.call_printf("%s", &[str_val.into()]);
                    },
                    8 => self.call_printf("%c", &[value.into()]),
                    64 => self.call_printf("%lld", &[value.into()]),
                    _ => self.call_printf("%d", &[value.into()]),
                }
            },

            BasicTypeEnum::FloatType(_) => self.call_printf("%.2f", &[value.into()]),

            _ => return Err(format!("unknown type for printing: {:?}", value.get_type()))
        }

        Ok(())
    }

    pub fn call_printf(&mut self, fmt: &str, args: &[BasicMetadataValueEnum<'c>]) {
        let printf = self.module.get_function("printf").expect("printf must be declared");
        let fmt_str = self.get_global_string_ptr(fmt);

        let mut final_args = vec![fmt_str.as_basic_value_enum().into()];
        final_args.extend_from_slice(args);

        self.builder.build_call(printf, &final_args, "printf_call");
    }

    pub fn get_global_string_ptr(&mut self, value: &str) -> PointerValue<'c> {
        if let Some(ptr) = self.string_constants.get(value) {
            return *ptr;
        }

        let ptr = self.builder.build_global_string_ptr(value, "str").as_pointer_value();
        self.string_constants.insert(value.to_string(), ptr);
        ptr
    }
}
