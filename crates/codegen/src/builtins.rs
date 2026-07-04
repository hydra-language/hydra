use inkwell::AddressSpace;
use inkwell::values::BasicValueEnum;
use mir::{Operand, MIRFunction};
use ir::types::Type;
use ir::Constant;
use crate::CodeGen;

impl<'c> CodeGen<'c> {

    /// Entry point called by `lower_terminator` in `stmts.rs`
    pub fn compile_builtin(&mut self, name: &str, args: &[Operand], mir_fn: &MIRFunction) -> Result<(), String> {
        match name {
            "println" => self.compile_print_base(args, true, mir_fn),
            "print"   => self.compile_print_base(args, false, mir_fn),
            _ => Err(format!("unknown builtin function: {}", name)),
        }
    }

    pub fn compile_print_base(&mut self, args: &[Operand], append_newline: bool, mir_fn: &MIRFunction) -> Result<(), String> {
        let fmt_str_op = args.first().ok_or("print requires format string")?;
        
        let fmt_str = match fmt_str_op {
            Operand::Const(Constant::String(s)) => s,
            _ => return Err("first arg must be a string literal".to_string())
        };

        let parts: Vec<&str> = fmt_str.split("{}").collect();
        let mut arg_iter = args.iter().skip(1);

        for (i, part) in parts.iter().enumerate() {
            if !part.is_empty() {
                self.call_print_str(part)?;
            }
            
            if i < parts.len() - 1 {
                let arg_op = arg_iter.next().ok_or("too few arguments for fmt string")?;
                let val = self.compile_operand(arg_op, mir_fn)?;
                let ty = self.get_operand_type(arg_op, mir_fn);
                self.compile_print_value(val, &ty)?;
            }
        }

        if append_newline {
            self.call_print_newline()?;
        }

        Ok(())
    }

    fn call_print_str(&mut self, s: &str) -> Result<(), String> {
        let void_type = self.context.void_type();
        let i64_type = self.context.i64_type();
        let i8_ptr_type = self.context.i8_type().ptr_type(AddressSpace::default());

        let fn_name = "print_str";
        let func = self.module.get_function(fn_name).unwrap_or_else(|| {
            let fn_type = void_type.fn_type(&[i8_ptr_type.into(), i64_type.into()], false);
            self.module.add_function(fn_name, fn_type, Some(inkwell::module::Linkage::External))
        });

        let ptr = self.get_global_string_ptr(s);
        let len = self.context.i64_type().const_int(s.len() as u64, false);

        self.builder.build_call(func, &[ptr.into(), len.into()], "call_print_str");
        Ok(())
    }

    fn call_print_newline(&mut self) -> Result<(), String> {
        let void_type = self.context.void_type();
        
        let fn_name = "print_newline";
        let func = self.module.get_function(fn_name).unwrap_or_else(|| {
            let fn_type = void_type.fn_type(&[], false);
            self.module.add_function(fn_name, fn_type, Some(inkwell::module::Linkage::External))
        });

        self.builder.build_call(func, &[], "call_print_newline");
        Ok(())
    }

    fn compile_print_value(&mut self, value: BasicValueEnum<'c>, ty: &Type) -> Result<(), String> {
        let void_type = self.context.void_type();
        let i64_type = self.context.i64_type();
        let bool_type = self.context.bool_type();

        match ty {
            Type::I64 | Type::ISIZE | Type::I32 | Type::I16 | Type::I8 => {
                let func = self.module.get_function("print_i64").unwrap_or_else(|| {
                    let fn_type = void_type.fn_type(&[i64_type.into()], false);
                    self.module.add_function("print_i64", fn_type, Some(inkwell::module::Linkage::External))
                });
                
                // Ensure value is expanded to 64-bit for the assembly call
                let extended = self.builder.build_int_s_extend(value.into_int_value(), i64_type, "sext_i64");
                self.builder.build_call(func, &[extended.into()], "call_print_i64");
            },

            Type::U64 | Type::USIZE | Type::U32 | Type::U16 | Type::U8 => {
                let func = self.module.get_function("print_u64").unwrap_or_else(|| {
                    let fn_type = void_type.fn_type(&[i64_type.into()], false);
                    self.module.add_function("print_u64", fn_type, Some(inkwell::module::Linkage::External))
                });
                
                let extended = self.builder.build_int_z_extend(value.into_int_value(), i64_type, "zext_u64");
                self.builder.build_call(func, &[extended.into()], "call_print_u64");
            },

            Type::BOOL => {
                let func = self.module.get_function("print_bool").unwrap_or_else(|| {
                    let fn_type = void_type.fn_type(&[bool_type.into()], false);
                    self.module.add_function("print_bool", fn_type, Some(inkwell::module::Linkage::External))
                });
                self.builder.build_call(func, &[value.into()], "call_print_bool");
            },

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
                         let len_value = self.context.i64_type().const_int(*size as u64, false);
                         
                         let func = self.module.get_function("print_str").unwrap_or_else(|| {
                             let fn_type = void_type.fn_type(&[i8_ptr_type.into(), i64_type.into()], false);
                             self.module.add_function("print_str", fn_type, Some(inkwell::module::Linkage::External))
                         });
                         
                         self.builder.build_call(func, &[str_ptr.into(), len_value.into()], "call_print_str");
                     },
                     _ => {
                         self.call_print_str("<array>")?;
                     }
                 }
            },

            Type::CHAR => {
                 let temp_alloca = self.builder.build_alloca(value.get_type(), "tmp_print_char"); 
                 self.builder.build_store(temp_alloca, value);
                 let i8_ptr_type = self.context.i8_type().ptr_type(AddressSpace::default());
                 let str_ptr = self.builder.build_bitcast(temp_alloca, i8_ptr_type, "str_ptr");
                 let len_value = self.context.i64_type().const_int(1, false);
                 
                 let func = self.module.get_function("print_str").unwrap_or_else(|| {
                     let fn_type = void_type.fn_type(&[i8_ptr_type.into(), i64_type.into()], false);
                     self.module.add_function("print_str", fn_type, Some(inkwell::module::Linkage::External))
                 });
                 self.builder.build_call(func, &[str_ptr.into(), len_value.into()], "call_print_char");
            },

            Type::POINTER(_) | Type::REF(_) | Type::CONST_REF(_) => {
                let ptr_to_int = self.builder.build_ptr_to_int(value.into_pointer_value(), i64_type, "ptr2int");
                let func = self.module.get_function("print_u64").unwrap_or_else(|| {
                    let fn_type = void_type.fn_type(&[i64_type.into()], false);
                    self.module.add_function("print_u64", fn_type, Some(inkwell::module::Linkage::External))
                });
                self.builder.build_call(func, &[ptr_to_int.into()], "call_print_ptr");
            },

            Type::F32 | Type::F64 => {
                 self.call_print_str("<float unsupported without libc>")?;
            },

            _ => {
                self.call_print_str("<unknown>")?;
            }
        }
        Ok(())
    }

    pub(crate) fn get_operand_type(&self, op: &Operand, mir_fn: &MIRFunction) -> Type {
        match op {
            Operand::Const(c) => match c {
                Constant::Int(_, ty) => ty.clone(),
                Constant::Float(_, ty) => ty.clone(),
                Constant::Bool(_) => Type::BOOL,
                Constant::Char(_) => Type::CHAR,
                Constant::String(_) => Type::POINTER(Box::new(Type::U8)),
            },
            Operand::Copy(place) |
            Operand::Move(place) => {
                // If it's a simple local, this is 100% accurate.
                mir_fn.locals[place.local.0].ty.clone()
            }
        }
    }
}
