use inkwell::{AddressSpace, FloatPredicate, types::BasicTypeEnum, values::{BasicValue, BasicValueEnum, PointerValue}};
use inkwell::types::BasicType;
use ir::{expr::{BinaryOp, Expr, ExprKind, UnaryOp}, types::Type};
use crate::{CodeGen, types::compile_type};

impl<'c> CodeGen<'c> {
    
    pub fn compile_expr(&mut self, expr: &Expr) -> Result<BasicValueEnum<'c>, String> {
        match &expr.kind {
            ExprKind::INT_LITERAL(val) => {
                let value = *val as u64;

                match expr.ty {
                    Type::I8 | Type::U8 | Type::CHAR => {
                        Ok(self.context.i8_type().const_int(value, false).into())
                    },
                    Type::I16 | Type::U16 => {
                        Ok(self.context.i16_type().const_int(value, false).into())
                    },
                    Type::I32 | Type::U32 => {
                        Ok(self.context.i32_type().const_int(value, false).into())
                    },
                    Type::I64 | Type::U64 | Type::ISIZE | Type::USIZE => {
                        Ok(self.context.i64_type().const_int(value, false).into())
                    },
                    Type::F32 => {
                        Ok(self.context.f32_type().const_float(*val as f64).into())
                    },
                    Type::F64 => {
                        Ok(self.context.f64_type().const_float(*val as f64).into())
                    },
                    Type::BOOL => {
                        Ok(self.context.bool_type().const_int(value, false).into())
                    },

                    _ => Err(format!("unsupported integer literal type: {:?}", expr.ty))
                }
            },

            ExprKind::FLOAT_LITERAL(val) => {
                match expr.ty {
                    Type::F32 => {
                        Ok(self.context.f32_type().const_float(*val).into())
                    },

                    Type::F64 => {
                        Ok(self.context.f64_type().const_float(*val).into())
                    },

                    _ => Err(format!("unsupported float literal type {:?}", expr.ty))
                }
            }

            ExprKind::STRING_LITERAL(s) => {
                Ok(self.context.const_string(s.as_bytes(), false).into())
            },

            ExprKind::CHAR_LITERAL(c) => {
                let value = *c as u64;
                Ok(self.context.i8_type().const_int(value, false).into())
            },

            ExprKind::Assignment { target, value, .. } => {
                let address = self.compile_target_address(target)?;
                let val = self.compile_expr(value)?;

                if let Type::STRUCT(_) | Type::ARRAY(_, _) = expr.ty {
                    if val.is_pointer_value() {
                        let src_ptr = val.into_pointer_value();
                        self.build_struct_copy(address, src_ptr, &expr.ty)?;
                    } else {
                        self.builder.build_store(address, val);
                    }
                } else {
                    self.builder.build_store(address, val);
                }

                Ok(val)
            },

            ExprKind::Cast { expr: inner_expr } => {
                let val = self.compile_expr(inner_expr)?;
                let src_ty = &inner_expr.ty;
                let dest_ty = &expr.ty;

                let dest_llvm_ty = compile_type(self.context, &self.target_data, dest_ty);

                match (src_ty, dest_ty) {
                    // --- 1. Float to Int (F64 -> I64/I32 etc) ---
                    (Type::F64 | Type::F32, Type::I64 | Type::I32 | Type::I16 | Type::I8 | Type::U8) => {
                        let float_val = val.into_float_value();

                        Ok(self.builder.build_float_to_signed_int(
                            float_val, 
                            dest_llvm_ty.into_int_type(), 
                            "cast_fptosi"
                        ).into())
                    },

                    // --- 2. Int to Float ---
                    (Type::I64 | Type::I32 | Type::I16 | Type::I8 | Type::U8, Type::F64 | Type::F32) => {
                        let int_val = val.into_int_value();

                        Ok(self.builder.build_signed_int_to_float(
                            int_val, 
                            dest_llvm_ty.into_float_type(), 
                            "cast_sitofp"
                        ).into())
                    },

                    // --- 3. Int to Int (Resizing) ---
                    (
                        Type::I64 | Type::U64 | Type::ISIZE | Type::USIZE | 
                        Type::I32 | Type::U32 | Type::CHAR | 
                        Type::I16 | Type::U16 | 
                        Type::I8 | Type::U8 | Type::BOOL,
                        
                        Type::I64 | Type::U64 | Type::ISIZE | Type::USIZE | 
                        Type::I32 | Type::U32 | Type::CHAR | 
                        Type::I16 | Type::U16 | 
                        Type::I8 | Type::U8 | Type::BOOL
                    ) => {
                        let int_val = val.into_int_value();
                        let dest_int_ty = dest_llvm_ty.into_int_type();
                        
                        let src_width = int_val.get_type().get_bit_width();
                        let dest_width = dest_int_ty.get_bit_width();

                        if src_width > dest_width {
                            Ok(self.builder.build_int_truncate(int_val, dest_int_ty, "cast_trunc").into())
                        } else if src_width < dest_width {
                            let is_unsigned = matches!(src_ty, 
                                Type::U8 | Type::U16 | Type::U32 | Type::U64 | 
                                Type::USIZE | Type::CHAR | Type::BOOL
                            );

                            if is_unsigned {
                                Ok(self.builder.build_int_z_extend(int_val, dest_int_ty, "cast_zext").into())
                            } else {
                                Ok(self.builder.build_int_s_extend(int_val, dest_int_ty, "cast_sext").into())
                            }
                        } else {
                            Ok(val) 
                        }
                    },

                    (Type::F64, Type::F32) => {
                        let float_val = val.into_float_value();

                        Ok(self.builder.build_float_trunc(
                            float_val, 
                            dest_llvm_ty.into_float_type(), 
                            "cast_fptrunc"
                        ).into())
                    },

                    (Type::F32, Type::F64) => {
                        let float_val = val.into_float_value();

                        Ok(self.builder.build_float_ext(
                            float_val, 
                            dest_llvm_ty.into_float_type(), 
                            "cast_fpext"
                        ).into())
                    },

                    (Type::REF(_) | Type::CONST_REF(_) | Type::POINTER(_), Type::POINTER(_)) => {
                        let ptr_val = val.into_pointer_value();

                        Ok(self.builder.build_pointer_cast(
                            ptr_val, 
                            dest_llvm_ty.into_pointer_type(), 
                            "cast_ptr"
                        ).into())
                    },

                    _ => Err(format!("codegen not implemented for cast: {} as {}", src_ty, dest_ty))
                }
            },

            ExprKind::ArrayInit { elements } => {
                let llvm_type = compile_type(self.context, &self.target_data, &expr.ty);
                let array_type = llvm_type.into_array_type();

                let mut compiled_elements = Vec::new();
                let mut all_const = true;

                for elem in elements {
                    let val = self.compile_expr(elem)?;
                    
                    if !self.is_val_const(&val) {
                        all_const = false;
                    }

                    compiled_elements.push(val);
                }

                if all_const {
                    let element_type = array_type.get_element_type();

                    let const_array = match element_type {
                        BasicTypeEnum::IntType(t) => {
                            let values: Vec<_> = compiled_elements.iter().map(|v| v.into_int_value()).collect();
                            t.const_array(&values).into()
                        },
                        BasicTypeEnum::FloatType(t) => {
                            let values: Vec<_> = compiled_elements.iter().map(|v| v.into_float_value()).collect();
                            t.const_array(&values).into()
                        },
                        BasicTypeEnum::PointerType(t) => {
                            let values: Vec<_> = compiled_elements.iter().map(|v| v.into_pointer_value()).collect();
                            t.const_array(&values).into()
                        },
                        BasicTypeEnum::ArrayType(t) => {
                            let values: Vec<_> = compiled_elements.iter().map(|v| v.into_array_value()).collect();
                            t.const_array(&values).into()
                        },
                        
                        _ => return Err("ICE: unsupported const array type element type".to_string())
                    };

                    Ok(const_array)
                } else {
                    let mut array_val = array_type.get_undef();

                    for (i, val) in compiled_elements.into_iter().enumerate() {
                        array_val = self.builder.build_insert_value(array_val, val, i as u32, "init")
                            .unwrap()
                            .into_array_value()
                    }

                    Ok(array_val.into())
                }
            },

            ExprKind::ArrayAccess { array, index } => {
                let arr_value = self.compile_expr(array)?;
                let arr_ptr = arr_value.into_pointer_value();

                let index_value = self.compile_expr(index)?;
                let index_int = index_value.into_int_value();

                let element_ptr = unsafe {
                    match array.ty {
                        Type::ARRAY(_, _) => {
                            let zero = self.context.i64_type().const_int(0, false);

                            self.builder.build_gep(
                                arr_ptr,
                                &[zero, index_int],
                                "array_idx",
                            )
                        },

                        Type::INFERRED_ARRAY(_) | Type::POINTER(_) => {
                            self.builder.build_gep(
                                arr_ptr,
                                &[index_int],
                                "ptr_idx"
                            )
                        },

                        _ => return Err(format!("ICE: cannot index type {:?}", array.ty))
                    }
                };

                Ok(self.builder.build_load(element_ptr, "elem_val"))
            }

            ExprKind::VariableReference { name } => {
                let ptr = if let Some(local) = self.variables.get(name) {
                    *local
                } else {
                    self.module.get_global(name)
                        .map(|g| g.as_pointer_value())
                        .ok_or_else(|| format!("ICE: analyzer failed to validate variable '{}'", name))?
                };

                if let Type::ARRAY(_, _) | Type::STRUCT(_) = expr.ty {
                    Ok(ptr.into())
                } else { 
                    Ok(self.builder.build_load(ptr, name)) 
                }
            },

            ExprKind::Binary { op, lhs, rhs } => {
                let left = self.compile_expr(lhs)?;
                let right = self.compile_expr(rhs)?;

                if left.is_float_value() {
                    let l = left.into_float_value();
                    let r = right.into_float_value();

                    let res: BasicValueEnum = match op {
                        BinaryOp::ADD => self.builder.build_float_add(l, r, "fadd").into(),
                        BinaryOp::SUB => self.builder.build_float_sub(l, r, "fsub").into(),
                        BinaryOp::MUL => self.builder.build_float_mul(l, r, "fmul").into(),
                        BinaryOp::DIV => self.builder.build_float_div(l, r, "fdiv").into(),
                        BinaryOp::LT  => self.builder.build_float_compare(FloatPredicate::OLT, l, r, "flt").into(),
                        BinaryOp::GT  => self.builder.build_float_compare(FloatPredicate::OGT, l, r, "fgt").into(),
                        BinaryOp::LE  => self.builder.build_float_compare(FloatPredicate::OLE, l, r, "fle").into(),
                        BinaryOp::GE  => self.builder.build_float_compare(FloatPredicate::OGE, l, r, "fge").into(),
                        BinaryOp::EQ  => self.builder.build_float_compare(FloatPredicate::OEQ, l, r, "feq").into(),
                        BinaryOp::NE  => self.builder.build_float_compare(FloatPredicate::ONE, l, r, "fne").into(),

                        _ => return Err(format!("Unsupported float binary op: {:?}", op)),
                    };

                    Ok(res)
                } else {
                    let l = left.into_int_value();
                    let r = right.into_int_value();

                    let res = match op {
                        BinaryOp::ADD => self.builder.build_int_add(l, r, "tmpadd"),
                        BinaryOp::SUB => self.builder.build_int_sub(l, r, "tmpsub"),
                        BinaryOp::MUL => self.builder.build_int_mul(l, r, "tmpmul"),
                        BinaryOp::DIV => self.builder.build_int_signed_div(l, r, "tmpdiv"),
                        BinaryOp::MOD => self.builder.build_int_signed_rem(l, r, "tmpmod"),

                        BinaryOp::LT => self.builder.build_int_compare(inkwell::IntPredicate::SLT, l, r, "tmplt"),                    
                        BinaryOp::LE => self.builder.build_int_compare(inkwell::IntPredicate::SLE, l, r, "tmple"),
                        BinaryOp::GT => self.builder.build_int_compare(inkwell::IntPredicate::SGT, l, r, "tmpgt"),
                        BinaryOp::GE => self.builder.build_int_compare(inkwell::IntPredicate::SGE, l, r, "tmpge"),
                        BinaryOp::EQ => self.builder.build_int_compare(inkwell::IntPredicate::EQ, l, r, "tmpeq"),
                        BinaryOp::NE => self.builder.build_int_compare(inkwell::IntPredicate::NE, l, r, "tmpne"),

                        BinaryOp::AND => self.builder.build_and(l, r, "tmpand"),
                        BinaryOp::OR  => self.builder.build_or(l, r, "tmpor"),
                    };

                    Ok(res.into())
                }
            },

            ExprKind::Unary { op, operand } => {
                match op {
                    UnaryOp::NEG => {
                        let val = self.compile_expr(operand)?;

                        if val.is_int_value() {
                            let int_value = val.into_int_value();
                            Ok(self.builder.build_int_neg(int_value, "neg").into())
                        } else if val.is_float_value() {
                            let float_value = val.into_float_value();
                            Ok(self.builder.build_float_neg(float_value, "neg").into())
                        } else {
                            Err("negation not supported on this type".to_string())
                        }
                    },

                    UnaryOp::NOT => {
                        let val = self.compile_expr(operand)?;

                        let int_value = val.into_int_value();
                        Ok(self.builder.build_not(int_value, "not").into())
                    },

                    UnaryOp::ADDR_OF => {
                        let ptr = self.compile_target_address(operand)?;

                        Ok(ptr.into())
                    },

                    UnaryOp::DEREF => {
                        let val = self.compile_expr(operand)?;
                        let ptr = val.into_pointer_value();

                        if let Type::STRUCT(_) | Type::ARRAY(_, _) = expr.ty {
                            Ok(ptr.into())
                        } else {
                            Ok(self.builder.build_load(ptr, "deref"))
                        }
                    }
                }
            }

            ExprKind::Call { callee, args, generic_args} => {
                if callee == "__intrinsic_layout_new" {
                    let target_type = generic_args.first().expect("Layout::new requires a generic type <T>");

                    let llvm_type = self.compile_type(target_type);

                    let size_val = llvm_type.size_of()
                        .expect("cannot calculate size of opaque type");

                    let layout_struct_type = self.module.get_struct_type("Layout")
                        .expect("struct Layout must be defined");

                    let mut layout_val = layout_struct_type.get_undef();

                    layout_val = self.builder.build_insert_value(layout_val, size_val, 0, "layout_init")
                        .unwrap().into_struct_value();

                    return Ok(layout_val.into());
                }

                if callee == "__intrinsic_alloc" {
                    let size_val = self.compile_expr(&args[0])?.into_int_value();

                    let malloc_fn = self.module.get_function("malloc").unwrap_or_else(|| {
                        let i8_ptr = self.context.i8_type().ptr_type(inkwell::AddressSpace::default());
                        let fn_type = i8_ptr.fn_type(&[self.context.i64_type().into()], false);

                        self.module.add_function("malloc", fn_type, None)
                    });

                    let result = self.builder.build_call(malloc_fn, &[size_val.into()], "malloc_ptr")
                        .try_as_basic_value().left().unwrap();

                    return Ok(result);
                }

                if callee == "__intrinsic_dealloc" {
                    let ptr_val = self.compile_expr(&args[0])?.into_pointer_value();

                    let free_fn = self.module.get_function("free").unwrap_or_else(|| {
                        let fn_type = self.context.void_type().fn_type(
                            &[self.context.i8_type().ptr_type(AddressSpace::default()).into()],
                            false
                        );
                        self.module.add_function("free", fn_type, None)
                    });

                    self.builder.build_call(free_fn, &[ptr_val.into()], "");

                    return Ok(self.context.i32_type().const_zero().into());
                }

                if callee == "println" {
                    return self.compile_println(args);
                }

                if callee == "print" {
                    return self.compile_print(args);
                }

                let func_value = self.module.get_function(callee)
                    .ok_or(format!("ICE: function '{}' not found in module", callee))?;

                let mut compiled_args = Vec::with_capacity(args.len());
                for arg in args {
                    let val = self.compile_expr(arg)?;
                    
                    if let Type::STRUCT(_) | Type::ARRAY(_, _) = arg.ty {
                        if val.is_pointer_value() {
                            let ptr = val.into_pointer_value();
                            compiled_args.push(self.builder.build_load(ptr, "arg_val").into());
                        } else {
                            compiled_args.push(val.into());
                        }
                    } else {
                        compiled_args.push(val.into());
                    }
                }

                let call_site = self.builder.build_call(func_value, &compiled_args, "tmpcall");

                match call_site.try_as_basic_value() {
                    item if item.is_left() => Ok(item.left().unwrap()),
                    _ => Ok(self.context.i32_type().const_zero().into()),
                }
            }

            ExprKind::StructInit { name, values } => {
                let struct_ty = self.context.get_struct_type(name)
                    .ok_or(format!("LLVM struct type {} not found", name))?;
                
                let ptr = self.builder.build_alloca(struct_ty, "struct_tmp");
                
                for (i, val_expr) in values.iter().enumerate() {
                    let llvm_val = self.compile_expr(val_expr)?;
                    
                    let field_ptr = self.builder.build_struct_gep(ptr, i as u32, "field_ptr")
                        .map_err(|_| "GEP failed during struct init")?;
                    
                    if let Type::STRUCT(_) = val_expr.ty {
                        if llvm_val.is_pointer_value() {
                            let src_ptr = llvm_val.into_pointer_value();
                            self.build_struct_copy(field_ptr, src_ptr, &val_expr.ty)?;
                        } else {
                            self.builder.build_store(field_ptr, llvm_val);
                        }
                    } else {
                        self.builder.build_store(field_ptr, llvm_val);
                    }
                }
                
                let final_val = self.builder.build_load(ptr, "struct_ret_val");
                
                Ok(final_val)
            },

            ExprKind::MemberAccess { .. } => {
                let field_ptr = self.compile_target_address(expr)?;

                if let Type::STRUCT(_) | Type::ARRAY(_, _) = expr.ty {
                    Ok(field_ptr.into())
                } else {
                    Ok(self.builder.build_load(field_ptr, "field_val"))
                }
            }

            _ => unimplemented!()
        }
    }

    pub fn compile_const_expr(&self, expr: &Expr, globals: &[(String, Type, Expr)]) -> Result<BasicValueEnum<'c>, String> {
        match &expr.kind {
            ExprKind::FLOAT_LITERAL(val) => {
                let f_ty = self.context.f64_type();
                Ok(f_ty.const_float(*val).into())
            }

            ExprKind::INT_LITERAL(val) => {
                let i_ty = self.context.i64_type();
                Ok(i_ty.const_int(*val as u64, false).into())
            }

            ExprKind::CHAR_LITERAL(c) => {
                let i_ty = self.context.i8_type();
                Ok(i_ty.const_int(*c as u64, false).into())
            }

            ExprKind::Binary { op, lhs, rhs } => {
                let left = self.compile_const_expr(lhs, globals)?;
                let right = self.compile_const_expr(rhs, globals)?;

                if left.is_float_value() {
                    let l = left.into_float_value();
                    let r = right.into_float_value();
                    match op {
                        BinaryOp::ADD => Ok(l.const_add(r).into()),
                        BinaryOp::SUB => Ok(l.const_sub(r).into()),
                        BinaryOp::MUL => Ok(l.const_mul(r).into()),
                        BinaryOp::DIV => Ok(l.const_div(r).into()),
                        _ => Err(format!("unsupported constant float op: {:?}", op)),
                    }
                } else {
                    let l = left.into_int_value();
                    let r = right.into_int_value();
                    match op {
                        BinaryOp::ADD => Ok(l.const_add(r).into()),
                        BinaryOp::SUB => Ok(l.const_sub(r).into()),
                        BinaryOp::MUL => Ok(l.const_mul(r).into()),
                        BinaryOp::MOD => Ok(l.const_signed_remainder(r).into()),
                        _ => Err(format!("unsupported constant int op: {:?}", op)),
                    }
                }
            }

            ExprKind::Unary { op, operand } => {
                let val = self.compile_const_expr(operand, globals)?;
                match op {
                    UnaryOp::NEG if val.is_float_value() => Ok(val.into_float_value().const_neg().into()),
                    UnaryOp::NEG if val.is_int_value() => Ok(val.into_int_value().const_neg().into()),
                    _ => Err("unsupported constant unary op".into()),
                }
            }

            ExprKind::StructInit { name, values } => {
                let struct_ty = self.context.get_struct_type(name)
                    .ok_or(format!("llvm struct type {} not found", name))?;

                let mut const_vals = Vec::new();
                for val in values {
                    const_vals.push(self.compile_const_expr(val, globals)?);
                }

                Ok(struct_ty.const_named_struct(&const_vals).into())
            }

            ExprKind::VariableReference { name } => {
                let global = self.module.get_global(name)
                    .ok_or_else(|| format!("global constant '{}' not found", name))?;

                if let Some(val) = global.get_initializer() {
                    return Ok(val);
                }

                let (_, _, init_expr) = globals.iter()
                    .find(|(n, _, _)| n == name)
                    .ok_or_else(|| format!("varaible '{}' is undefined", name))?;

                self.compile_const_expr(init_expr, globals)
            }

            ExprKind::ArrayInit { elements } => {
                let llvm_type = compile_type(self.context, &self.target_data, &expr.ty);
                let array_type = llvm_type.into_array_type();

                let mut compiled = Vec::new();
                for e in elements {
                    compiled.push(self.compile_const_expr(e, globals)?);
                }

                match array_type.get_element_type() {
                    BasicTypeEnum::FloatType(t) => {
                        let vals: Vec<_> = compiled.iter().map(|v| v.into_float_value()).collect();
                        Ok(t.const_array(&vals).into())
                    }
                    BasicTypeEnum::IntType(t) => {
                        let vals: Vec<_> = compiled.iter().map(|v| v.into_int_value()).collect();
                        Ok(t.const_array(&vals).into())
                    }
                    _ => Err("unsupported constant array element type".into())
                }
            }

            _ => Err(format!("expression type {:?} is not a valid global constant", expr.kind))
        }
    }

    pub fn compile_type(&self, ty: &Type) -> BasicTypeEnum<'c> {
        match ty {
            Type::I32 => self.context.i32_type().into(),
            Type::I64 => self.context.i64_type().into(),
            Type::F64 => self.context.f64_type().into(),
            Type::BOOL => self.context.bool_type().into(),
            
            Type::REF(inner) | Type::CONST_REF(inner) => {
                let inner_llvm = self.compile_type(inner);
                inner_llvm.ptr_type(inkwell::AddressSpace::default()).into()
            },
            
            Type::STRUCT(name) => {
                let struct_ty = self.module.get_struct_type(name).expect("struct not found");
                struct_ty.into()
            },

             _ => panic!("type compilation not implemented for {:?}", ty),
        }
    }

    pub fn compile_target_address(&mut self, expr: &Expr) -> Result<inkwell::values::PointerValue<'c>, String> {
        match &expr.kind {
            ExprKind::VariableReference { name } => {
                let ptr = self.variables.get(name)
                    .ok_or_else(|| format!("Variable '{}' not found", name))?;

                if let Type::REF(_) | Type::CONST_REF(_) = expr.ty {
                    Ok(self.builder.build_load(*ptr, &format!("{}_deref", name)).into_pointer_value())
                } else {
                    Ok(*ptr)
                }
            },

            ExprKind::MemberAccess { object, index, .. } => {
                let obj_ptr = self.compile_target_address(object)?;

                let field_ptr = self.builder.build_struct_gep(obj_ptr, *index, "field_ptr")
                    .map_err(|e| format!("GEP failed at index {}: {:?}", index, e))?;

                Ok(field_ptr)
            },

            ExprKind::ArrayAccess { array, index } => {
                let arr_value = self.compile_expr(array)?;
                let arr_ptr = arr_value.into_pointer_value();

                let index_value = self.compile_expr(index)?;
                let index_int = index_value.into_int_value();

                unsafe {
                    match array.ty {
                        Type::ARRAY(_, _) => {
                            let zero = self.context.i64_type().const_int(0, false);
                            Ok(self.builder.build_gep(
                                arr_ptr,
                                &[zero, index_int],
                                "array_idx_ptr",
                            ))
                        },

                        Type::INFERRED_ARRAY(_) | Type::POINTER(_) => {
                            Ok(self.builder.build_gep(
                                arr_ptr,
                                &[index_int],
                                "ptr_idx_ptr"
                            ))
                        },

                        _ => Err(format!("ICE: cannot take address of index for type {:?}", array.ty))
                    }
                }
            },

            _ => {
                let val = self.compile_expr(expr)?;

                let is_aggregate = matches!(expr.ty, Type::STRUCT(_) | Type::ARRAY(_, _));

                if is_aggregate && val.is_pointer_value() {
                    Ok(val.into_pointer_value())
                } else {
                    let llvm_ty = compile_type(self.context, &self.target_data, &expr.ty);
                    let alloca = self.builder.build_alloca(llvm_ty, "tmp_rval");

                    self.builder.build_store(alloca, val);
                    Ok(alloca)
                }
            }
        }
    }

    fn build_struct_copy(&self, dest: PointerValue<'c>, src: PointerValue<'c>, ty: &Type) 
        -> Result<(), String> 
    {
        // 1. Convert Hydra Type to LLVM Type
        let llvm_type = compile_type(self.context, &self.target_data, ty);

        // 2. Get size in bytes from TargetData
        let size_value = unsafe {
            self.builder.build_gep(
                llvm_type.ptr_type(inkwell::AddressSpace::from(0)).const_null(),
                &[self.context.i64_type().const_int(1, false)],
                "size_calc"
            ).const_to_int(self.context.i64_type())
        };

        // 3. Perform a 8-byte aligned copy (standard for f64 vectors)
        self.builder.build_memcpy(dest, 8, src, 8, size_value)
            .map_err(|_| "internal llvm error: memcpy failed".to_string())?;

        Ok(())
    }
}
