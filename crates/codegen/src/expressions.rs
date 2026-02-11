use inkwell::{types::BasicTypeEnum, values::{BasicValue, BasicValueEnum}};
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

            ExprKind::STRING_LITERAL(s) => {
                Ok(self.context.const_string(s.as_bytes(), false).into())
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
                let ptr = self.variables.get(name)
                    .expect(&format!("ICE: analyzer failed to validate variable '{}'", name));

                if let Type::ARRAY(_, _) = expr.ty {
                    Ok(ptr.as_basic_value_enum())
                } else { 
                    Ok(self.builder.build_load(*ptr, name)) 
                }
            },

            ExprKind::Binary { op, lhs, rhs } => {
                let left = self.compile_expr(lhs)?;
                let right = self.compile_expr(rhs)?;

                let lhs_val = left.into_int_value();
                let rhs_val = right.into_int_value();

                let result = match op {
                    BinaryOp::ADD => self.builder.build_int_add(lhs_val, rhs_val, "tmpadd"),
                    BinaryOp::SUB => self.builder.build_int_sub(lhs_val, rhs_val, "tmpsub"),
                    BinaryOp::MUL => self.builder.build_int_mul(lhs_val, rhs_val, "tmpmul"),
                    BinaryOp::DIV => self.builder.build_int_signed_div(lhs_val, rhs_val, "tmpdiv"),

                    _ => return Err("binary op not yet supported".to_string())
                };

                Ok(result.into())
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
                    compiled_args.push(self.compile_expr(arg)?.into());
                }

                let call_site = self.builder.build_call(func_value, &compiled_args, "tmpcall");

                match call_site.try_as_basic_value() {
                    item if item.is_left() => Ok(item.left().unwrap()),
                    _ => Ok(self.context.i32_type().const_zero().into()),
                }
            }

            _ => unimplemented!()
        }
    }
}
