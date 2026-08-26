use inkwell::values::{BasicValue, BasicValueEnum};
use inkwell::types::{BasicType, BasicTypeEnum};
use ir::Constant;
use ir::hir::{CastKind, HIRBinOp, HIRUnaryOp};
use ir::types::Type;
use ir::intrinsic::IntrinsicKind;
use mir::{Rvalue, Operand, AggregateKind, MIRFunction};
use crate::CodeGen;

impl<'c> CodeGen<'c> {

    pub fn compile_rvalue(&mut self, rvalue: &Rvalue, mir_fn: &MIRFunction) -> Result<BasicValueEnum<'c>, String> {
        match rvalue {
            Rvalue::Use(op) => self.compile_operand(op, mir_fn),
            Rvalue::Ref(_, place) => {
                let ptr = self.compile_place(place, mir_fn)?;
                Ok(ptr.into())
            }

            Rvalue::UnaryOp(op, operand) => {
                let val = self.compile_operand(operand, mir_fn)?;
                match op {
                    HIRUnaryOp::Neg => {
                        if val.is_float_value() {
                            Ok(self.builder.build_float_neg(val.into_float_value(), "neg").into())
                        } else {
                            Ok(self.builder.build_int_neg(val.into_int_value(), "neg").into())
                        }
                    }

                    HIRUnaryOp::Not => {
                        Ok(self.builder.build_not(val.into_int_value(), "not").into())
                    }

                    HIRUnaryOp::AddrOf => {
                        Err("ICE: AddrOf should be lowered to Rvalue::Ref, not UnaryOp".to_string())
                    }

                    HIRUnaryOp::Deref => {
                        Err("ICE: Deref should be lowered to ProjectionElem::Deref, not UnaryOp".to_string())
                    }
                }
            }

            Rvalue::BinaryOp(op, lhs, rhs) => {
                let left = self.compile_operand(lhs, mir_fn)?;
                let right = self.compile_operand(rhs, mir_fn)?;

                if left.is_float_value() {
                    let l = left.into_float_value();
                    let r = right.into_float_value();

                    match op {
                        HIRBinOp::Add => Ok(self.builder.build_float_add(l, r, "fadd").into()),
                        HIRBinOp::Sub => Ok(self.builder.build_float_sub(l, r, "fsub").into()),
                        HIRBinOp::Mul => Ok(self.builder.build_float_mul(l, r, "fmul").into()),
                        HIRBinOp::Div => Ok(self.builder.build_float_div(l, r, "fdiv").into()),
                        HIRBinOp::Eq  => Ok(self.builder.build_float_compare(inkwell::FloatPredicate::OEQ, l, r, "feq").into()),
                        HIRBinOp::Ne  => Ok(self.builder.build_float_compare(inkwell::FloatPredicate::ONE, l, r, "fne").into()),
                        HIRBinOp::Lt  => Ok(self.builder.build_float_compare(inkwell::FloatPredicate::OLT, l, r, "flt").into()),
                        HIRBinOp::Le  => Ok(self.builder.build_float_compare(inkwell::FloatPredicate::OLE, l, r, "fle").into()),
                        HIRBinOp::Gt  => Ok(self.builder.build_float_compare(inkwell::FloatPredicate::OGT, l, r, "fgt").into()),
                        HIRBinOp::Ge  => Ok(self.builder.build_float_compare(inkwell::FloatPredicate::OGE, l, r, "fge").into()),
                        _ => Err(format!("unsupported float binary op: {:?}", op)),
                    }
                } else {
                    let l = left.into_int_value();
                    let r = right.into_int_value();

                    match op {
                        HIRBinOp::Add => Ok(self.builder.build_int_add(l, r, "add").into()),
                        HIRBinOp::Sub => Ok(self.builder.build_int_sub(l, r, "sub").into()),
                        HIRBinOp::Mul => Ok(self.builder.build_int_mul(l, r, "mul").into()),
                        HIRBinOp::Div => Ok(self.builder.build_int_signed_div(l, r, "div").into()),
                        HIRBinOp::Mod => Ok(self.builder.build_int_signed_rem(l, r, "rem").into()),
                        HIRBinOp::Eq  => Ok(self.builder.build_int_compare(inkwell::IntPredicate::EQ, l, r, "eq").into()),
                        HIRBinOp::Ne  => Ok(self.builder.build_int_compare(inkwell::IntPredicate::NE, l, r, "ne").into()),
                        HIRBinOp::Lt  => Ok(self.builder.build_int_compare(inkwell::IntPredicate::SLT, l, r, "lt").into()),
                        HIRBinOp::Le  => Ok(self.builder.build_int_compare(inkwell::IntPredicate::SLE, l, r, "le").into()),
                        HIRBinOp::Gt  => Ok(self.builder.build_int_compare(inkwell::IntPredicate::SGT, l, r, "gt").into()),
                        HIRBinOp::Ge  => Ok(self.builder.build_int_compare(inkwell::IntPredicate::SGE, l, r, "ge").into()),
                        HIRBinOp::And => Ok(self.builder.build_and(l, r, "and").into()),
                        HIRBinOp::Or  => Ok(self.builder.build_or(l, r, "or").into()),
                    }
                }
            }

            Rvalue::Cast(kind, operand, dest_ty) => {
                let val = self.compile_operand(operand, mir_fn)?;
                let dest_llvm_ty = crate::types::compile_type(self.context, &self.target_data, dest_ty);
                let src_ty = self.get_operand_type(operand, mir_fn);

                match kind {
                    CastKind::NoOp => Ok(val),
                    
                    CastKind::Pointer => {
                        Ok(self.builder.build_pointer_cast(
                            val.into_pointer_value(),
                            dest_llvm_ty.into_pointer_type(),
                            "ptr_cast"
                        ).into())
                    }

                    CastKind::Numeric => {
                        match (src_ty.clone(), dest_ty) {
                            // Float to Int
                            (Type::F32 | Type::F64, Type::I8 | Type::I16 | Type::I32 | Type::I64 | Type::U8 | Type::U16 | Type::U32 | Type::U64) => {
                                Ok(self.builder.build_float_to_signed_int(
                                    val.into_float_value(), dest_llvm_ty.into_int_type(), "fptosi"
                                ).into())
                            }
                            // Int to Float
                            (Type::I8 | Type::I16 | Type::I32 | Type::I64 | Type::U8 | Type::U16 | Type::U32 | Type::U64, Type::F32 | Type::F64) => {
                                Ok(self.builder.build_signed_int_to_float(
                                    val.into_int_value(), dest_llvm_ty.into_float_type(), "sitofp"
                                ).into())
                            }
                            // Int to Int (Resize)
                            (src, dest) if src.is_numeric() && dest.is_numeric() => {
                                let int_val = val.into_int_value();
                                let dest_int_ty = dest_llvm_ty.into_int_type();
                                let src_width = int_val.get_type().get_bit_width();
                                let dest_width = dest_int_ty.get_bit_width();

                                if src_width > dest_width {
                                    Ok(self.builder.build_int_truncate(int_val, dest_int_ty, "trunc").into())
                                } else {
                                    let is_unsigned = matches!(src, Type::U8 | Type::U16 | Type::U32 | Type::U64 | Type::USIZE | Type::CHAR | Type::BOOL);
                                    if is_unsigned {
                                        Ok(self.builder.build_int_z_extend(int_val, dest_int_ty, "zext").into())
                                    } else {
                                        Ok(self.builder.build_int_s_extend(int_val, dest_int_ty, "sext").into())
                                    }
                                }
                            }
                            // Float to Float (Resize)
                            (Type::F32, Type::F64) => Ok(self.builder.build_float_ext(val.into_float_value(), dest_llvm_ty.into_float_type(), "fpext").into()),
                            (Type::F64, Type::F32) => Ok(self.builder.build_float_trunc(val.into_float_value(), dest_llvm_ty.into_float_type(), "fptrunc").into()),
                            _ => Err(format!("Unsupported cast from {:?} to {:?}", src_ty, dest_ty)),
                        }
                    }
                }
            }


            Rvalue::Aggregate(kind, operands) => {
                let llvm_ty: BasicTypeEnum = match kind {
                    AggregateKind::Struct(def_id) => {
                        let info = self.hir_context.get_def(*def_id).expect("ICE: struct definition not found");

                        let struct_name = if info.absolute_path.is_empty() {
                            info.name.clone()
                        } else {
                            info.absolute_path.join("::")
                        };

                        self.module
                            .get_struct_type(&struct_name)
                            .unwrap_or_else(|| {
                                panic!(
                                    "ICE: struct '{}' not registered in module",
                                    struct_name
                                )
                            })
                            .as_basic_type_enum()
                    }

                    AggregateKind::Array(inner) => {
                        crate::types::compile_type(self.context, &self.target_data, inner)
                            .array_type(operands.len() as u32).into()
                    }
                };

                let mut agg_val = match llvm_ty {
                    BasicTypeEnum::ArrayType(t) => t.get_undef().as_basic_value_enum(),
                    BasicTypeEnum::StructType(t) => t.get_undef().as_basic_value_enum(),
                    _ => return Err("ICE: only arrays and structs are supported as aggregates".to_string()),
                };

                for (i, op) in operands.iter().enumerate() {
                    let op_val = self.compile_operand(op, mir_fn)?;
                    
                    agg_val = match agg_val {
                        BasicValueEnum::ArrayValue(arr) => self.builder
                            .build_insert_value(arr, op_val, i as u32, "init")
                            .unwrap().into_array_value().as_basic_value_enum(),
                        BasicValueEnum::StructValue(strct) => self.builder
                            .build_insert_value(strct, op_val, i as u32, "init")
                            .unwrap().into_struct_value().as_basic_value_enum(),
                        _ => unreachable!(),
                    };
                }
                
                Ok(agg_val)
            }

            Rvalue::Intrinsic { kind, type_args, args, .. } => {
                self.compile_intrinsic(*kind, type_args, args, mir_fn)
            }
        }
    }

    pub fn compile_operand(&mut self, operand: &Operand, mir_fn: &MIRFunction) -> Result<BasicValueEnum<'c>, String> {
        match operand {
            Operand::Copy(place) | Operand::Move(place) => {
                let ptr = self.compile_place(place, mir_fn)?;
                Ok(self.builder.build_load(ptr, "op_load"))
            }

            Operand::Const(c) => {
                match c {
                    Constant::Int(v, ty) => {
                        let llvm_ty = match ty {
                            Type::I8  | Type::U8  => self.context.i8_type(),
                            Type::I16 | Type::U16 => self.context.i16_type(),
                            Type::I32 | Type::U32 => self.context.i32_type(),
                            Type::I64 | Type::U64 | Type::ISIZE | Type::USIZE => self.context.i64_type(),
                            // fallback for untyped/inferred int literals
                            _ => self.context.i32_type(),
                        };

                        Ok(llvm_ty.const_int(*v as u64, false).into())
                    }

                    Constant::Float(v, ty) => {
                        let llvm_ty = match ty {
                            Type::F32 => self.context.f32_type(),
                            _ => self.context.f64_type(),
                        };

                        Ok(llvm_ty.const_float(*v).into())
                    }

                    Constant::Bool(v) => {
                        Ok(self.context.bool_type().const_int(if *v { 1 } else { 0 }, false).into())
                    }

                    Constant::String(s) => {
                        Ok(self.get_global_string_ptr(s).into())
                    }

                    Constant::Char(c) => {
                        Ok(self.context.i8_type().const_int(*c as u64, false).into())
                    }
                }
            }
        }
    }
}
