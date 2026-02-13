use inkwell::{FloatPredicate, types::BasicTypeEnum, values::{BasicValue, BasicValueEnum, PointerValue}};
use inkwell::types::BasicType;
use ir::{expr::{BinaryOp, Expr, ExprKind, UnaryOp}, types::Type};
use crate::{CodeGen, types::compile_type};

impl<'c> CodeGen<'c> {
    
    pub fn compile_expr(&mut self, expr: &Expr) -> Result<BasicValueEnum<'c>, String> {
        match &expr.kind {
            ExprKind::INT_LITERAL(val) => {
                let value = *val as u64;

                match expr.ty {
                    Type::I8 | Type::U8 => {
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
                    Type::BOOL => {
                        Ok(self.context.bool_type().const_int(value, false).into())
                    },
                    Type::CHAR => {
                        Ok(self.context.i32_type().const_int(value, false).into())
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

            ExprKind::Assignment { target, value, .. } => {
                let address = self.compile_target_address(target)?;
                let val = self.compile_expr(value)?;

                if let Type::STRUCT(_) = expr.ty {
                    let src_ptr = val.into_pointer_value();
                    self.build_struct_copy(address, src_ptr, &expr.ty)?;
                } else {
                    self.builder.build_store(address, val);
                }

                Ok(val)
            }

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
                let ptr = self.variables.get(name)
                    .expect(&format!("ICE: analyzer failed to validate variable '{}'", name));

                if let Type::ARRAY(_, _) | Type::STRUCT(_) = expr.ty {
                    Ok((*ptr).into())
                } else { 
                    Ok(self.builder.build_load(*ptr, name)) 
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
                let val = self.compile_expr(operand)?;

                match op {
                    UnaryOp::NEG => {
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
                        let int_value = val.into_int_value();
                        Ok(self.builder.build_not(int_value, "not").into())
                    },

                    UnaryOp::ADDR_OF => {
                        match &operand.kind {
                            ExprKind::VariableReference { name } => {
                                let ptr = self.get_variable_pointer(name); 
                                let value: BasicValueEnum = ptr.into();

                                Ok(value)
                            }
                            
                             _ => Err("can only take address of a variable".to_string())
                        }
                    }
                }
            }

            ExprKind::Call { callee, args } => {
                if callee == "println" {
                    return self.compile_println(args);
                }

                let func_value = self.module.get_function(callee)
                    .ok_or(format!("ICE: function '{}' not found in module", callee))?;

                let mut compiled_args = Vec::with_capacity(args.len());
                for arg in args {
                    let val = self.compile_expr(arg)?;
                    
                    if let Type::STRUCT(_) = arg.ty {
                        let ptr = val.into_pointer_value();
                        compiled_args.push(self.builder.build_load(ptr, "arg_val").into());
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
                
                // Create memory on the stack for the new instance
                let ptr = self.builder.build_alloca(struct_ty, "struct_tmp");
                
                // Fill the memory with field values
                for (i, val_expr) in values.iter().enumerate() {
                    let llvm_val = self.compile_expr(val_expr)?;
                    
                    // Get pointer to field at index i
                    let field_ptr = self.builder.build_struct_gep(ptr, i as u32, "field_ptr")
                        .map_err(|_| "GEP failed during struct init")?;
                    
                    if let Type::STRUCT(_) = val_expr.ty {
                        let src_ptr = llvm_val.into_pointer_value();
                        self.build_struct_copy(field_ptr, src_ptr, &val_expr.ty)?;
                    } else {
                        self.builder.build_store(field_ptr, llvm_val);
                    }
                }

                let final_val = self.builder.build_load(ptr, "struct_ret_val");

                Ok(final_val)
            },

            ExprKind::MemberAccess { .. } => {
                let field_ptr = self.compile_target_address(expr)?;

                // THE FIX: If we are still looking at a struct, return the pointer.
                // If it's a final field (f64), load it.
                if let Type::STRUCT(_) = expr.ty {
                    Ok(field_ptr.into())
                } else {
                    Ok(self.builder.build_load(field_ptr, "field_val"))
                }
            }

            _ => unimplemented!()
        }
    }

    pub fn compile_target_address(&mut self, expr: &Expr) -> Result<inkwell::values::PointerValue<'c>, String> {
        match &expr.kind {
            ExprKind::VariableReference { name } => {
                let ptr = self.variables.get(name)
                    .ok_or_else(|| format!("Variable '{}' not found", name))?;

                // Check if the variable is a reference type (like 'self' or an '&' arg).
                // If it is a reference, the pointer in our map is a "pointer to a pointer."
                // We must load the actual address before we can GEP into it.
                if let Type::REF(_) | Type::CONST_REF(_) = expr.ty {
                    Ok(self.builder.build_load(*ptr, &format!("{}_deref", name)).into_pointer_value())
                } else {
                    Ok(*ptr)
                }
            },

            ExprKind::MemberAccess { object, index, .. } => {
                let obj_ptr = self.compile_target_address(object)?;

                // IMPORTANT: LLVM requires the pointer to be a Pointer to a Struct.
                // If this fails, it's usually because 'object' returned a pointer to a primitive.
                let field_ptr = self.builder.build_struct_gep(obj_ptr, *index, "field_ptr")
                    .map_err(|e| format!("GEP failed at index {}: {:?}", index, e))?;

                Ok(field_ptr)
            },
            _ => Err(format!("Cannot get address of {:?}", expr.kind)),
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
