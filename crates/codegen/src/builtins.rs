use inkwell::AddressSpace;
use inkwell::values::{BasicMetadataValueEnum, BasicValueEnum, BasicValue, FunctionValue, PointerValue};

use ir::expr::{Expr, ExprKind};
use ir::types::Type;
use crate::CodeGen;

impl<'c> CodeGen<'c> {


    pub fn compile_println(&mut self, args: &[Expr]) -> Result<BasicValueEnum<'c>, String> 
    {
        let fmt_str_expr = args.first().ok_or("println requires format string")?;

        let fmt_str = match &fmt_str_expr.kind {
            ExprKind::STRING_LITERAL(s) => s,
            _ => return Err("first arg must be a string literal".to_string())
        };

        let parts: Vec<&str> = fmt_str.split("{}").collect();
        let mut arg_iter = args.iter().skip(1);

        for (i, part) in parts.iter().enumerate() {
            if !part.is_empty() {
                let part_global = self.get_global_string_ptr(part);
                self.call_printf("%s", &[part_global.into()]);
            }

            if i < parts.len() - 1 {
                let arg_expr = arg_iter.next()
                    .ok_or("too few arguments for fmt string")?;

                let val = self.compile_expr(arg_expr)?;

                self.compile_print_value(val, &arg_expr.ty)?;
            }
        }

        let newline = self.get_global_string_ptr("\n");

        self.call_printf("%s", &[newline.into()]);

        Ok(self.context.i32_type().const_zero().into())
    }

    fn compile_print_value(&mut self, value: BasicValueEnum<'c>, ty: &Type) -> Result<(), String> {
        match ty {
            Type::I32 => self.call_printf("%d", &[value.into()]),
            Type::U32 => self.call_printf("%u", &[value.into()]),
            Type::F32 | Type::F64 => self.call_printf("%f", &[value.into()]),
            Type::I64 | Type::ISIZE => self.call_printf("%lld", &[value.into()]),
            Type::U64 | Type::USIZE => self.call_printf("%llu", &[value.into()]),
            Type::I8 | Type::U8 | Type::CHAR => self.call_printf("%c", &[value.into()]),

            Type::ARRAY(inner, size) => {
                match **inner {
                    Type::U8 | Type::I8 => {
                        let ptr = if value.is_pointer_value() {
                            value.into_pointer_value()
                        } else {
                            let temp_alloca = self.builder.build_alloca(value.get_type(), "tmp_print_str");
                            self.builder.build_store(temp_alloca, value);

                            temp_alloca
                        };

                        let i8_ptr_type = self.context.i8_type().ptr_type(AddressSpace::default());
                        let str_ptr = self.builder.build_bitcast(ptr, i8_ptr_type, "str_ptr");

                        let len_value = self.context.i32_type().const_int(*size as u64, false);

                        let args: Vec<BasicMetadataValueEnum> = vec![
                            len_value.into(),
                            str_ptr.into()
                        ];

                        self.call_printf("%.*s", &args);
                    },

                    _ => self.call_printf("<array>", &[]),
                }
            },

            Type::BOOL => {
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

            _ => self.call_printf("%d", &[value.into()]),
        }

        Ok(())
    }

    pub fn call_printf(&mut self, fmt: &str, args: &[BasicMetadataValueEnum<'c>]) {
        let printf = self.get_printf_declaration();
        let fmt_str = self.get_global_string_ptr(fmt);

        let mut final_args = vec![fmt_str.as_basic_value_enum().into()];
        final_args.extend_from_slice(args);

        self.builder.build_call(printf, &final_args, "printf_call");
    }

    pub fn get_printf_declaration(&self) -> FunctionValue<'c> {
        if let Some(function) = self.module.get_function("printf") {
            return function;
        }

        let i32_type = self.context.i32_type();
        let i8_ptr_type = self.context.i8_type().ptr_type(AddressSpace::default());
        let printf_type = i32_type.fn_type(&[i8_ptr_type.into()], true);

        self.module.add_function("printf", printf_type, None)
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
