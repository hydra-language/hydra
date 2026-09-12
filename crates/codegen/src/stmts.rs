use crate::CodeGen;
use ir::{context::DefKind, types::Type};
use mir::{Statement, StatementKind, Terminator, Place, MIRFunction, LocalID};
use inkwell::{IntPredicate, values::{IntValue, PointerValue}};

impl<'c> CodeGen<'c> {

    pub fn compile_stmt(&mut self, stmt: &Statement, mir_fn: &MIRFunction) -> Result<(), String> {
        match &stmt.kind {
            StatementKind::Assign(place, rvalue) => {
                let rval_llvm = self.compile_rvalue(rvalue, mir_fn)?;

                // Check type of local being assigned to
                let place_ty = &mir_fn.locals[place.local.0].ty;
                if *place_ty != ir::types::Type::VOID {
                    let dest_ptr = self.compile_place(place, mir_fn)?;
                    self.builder.build_store(dest_ptr, rval_llvm);
                }
                // If it IS void, we do nothing (no memory to store into!)
            }

            StatementKind::Drop(place) => {
                let ty = &mir_fn.locals[place.local.0].ty;

                let Type::STRUCT(type_name) = ty else {
                    return Ok(());
                };

                let Some(drop_def_id) = self.hir_context.get_drop_impl(&type_name.symbol) else {
                    return Ok(());
                };

                let drop_info = self.hir_context.get_def(drop_def_id).ok_or_else(|| {
                    format!(
                        "ICE: missing drop implementation for `{}`",
                        type_name
                    )
                })?;

                let drop_name = if drop_info.absolute_path.is_empty() {
                    drop_info.name.clone()
                } else {
                    drop_info.absolute_path.join("::")
                };

                let drop_fn = self.module.get_function(&drop_name).ok_or_else(|| {
                    format!(
                        "ICE: LLVM drop function `{}` not found",
                        drop_name
                    )
                })?;

                //
                // drop(&mut self)
                //
                // compile_place() gives us the address of the Box local,
                // which is exactly the representation expected by &mut self.
                //
                let self_ptr = self.compile_place(place, mir_fn)?;

                self.builder.build_call(
                    drop_fn,
                    &[self_ptr.into()],
                    "",
                );
            }
        }

        Ok(())
    }

    pub fn compile_terminator(&mut self, term: &Terminator, mir_fn: &MIRFunction) -> Result<(), String> {
        match term {
            Terminator::Goto { target } => {
                let target_bb = self.blocks.get(target).unwrap();
                self.builder.build_unconditional_branch(*target_bb);
            }
            Terminator::SwitchInt { discriminant, true_target, false_target } => {
                let cond_val = self.compile_operand(discriminant, mir_fn)?.into_int_value();
                let t_bb = self.blocks.get(true_target).unwrap();
                let f_bb = self.blocks.get(false_target).unwrap();
                self.builder.build_conditional_branch(cond_val, *t_bb, *f_bb);
            }
            Terminator::Return => {
                let ret_ty = &mir_fn.locals[0].ty;
                if *ret_ty == ir::types::Type::VOID {
                    self.builder.build_return(None);
                } else {
                    let ret_ptr = self.locals.get(&LocalID(0)).unwrap();
                    let ret_val = self.builder.build_load(*ret_ptr, "ret_val");
                    self.builder.build_return(Some(&ret_val));
                }
            }

            Terminator::Call { callee, args, destination, target } => {
                let func_info = self.module.get_function(&callee.symbol)
                    .ok_or_else(|| format!("ICE: no LLVM function found for '{}'", callee))?;

                let mut llvm_args = Vec::new();
                for arg in args {
                    llvm_args.push(self.compile_operand(arg, mir_fn)?.into());
                }

                let call_val = self.builder.build_call(func_info, &llvm_args, "call_tmp");

                let dest_ty = &mir_fn.locals[destination.local.0].ty;
                if *dest_ty != ir::types::Type::VOID {
                    let dest_ptr = self.compile_place(destination, mir_fn)?;
                    if let Some(val) = call_val.try_as_basic_value().left() {
                        self.builder.build_store(dest_ptr, val);
                    }
                }

                let target_bb = self.blocks.get(target).unwrap();
                self.builder.build_unconditional_branch(*target_bb);
            }

            Terminator::BuiltinCall { name, args, target } => {
                self.compile_builtin(name, args, mir_fn)?;
                let target_bb = self.blocks.get(target).unwrap();
                self.builder.build_unconditional_branch(*target_bb);
            }
            Terminator::Unreachable => {
                self.builder.build_unreachable();
            }
        }
        Ok(())
    }

    /// Evaluates a Place down to an LLVM Memory Pointer
    pub fn compile_place(&self, place: &Place, mir_fn: &MIRFunction) -> Result<PointerValue<'c>, String> {
        let ptr = self.locals.get(&place.local).ok_or_else(|| {
            format!("ICE: Attempted to compile place for unallocated local _{}", place.local.0)
        })?;

        let mut ptr = *ptr;
        let mut current_ty = mir_fn.locals[place.local.0].ty.clone();
        let mut slice_len: Option<IntValue<'c>> = None;

        for proj in &place.projection {
            match proj {
                mir::ProjectionElem::Deref => {
                    match &current_ty {
                        Type::REF(inner) | Type::CONST_REF(inner) if matches!(inner.as_ref(), Type::SLICE(_)) => {
                            //
                            // a slice reference is { ptr, len }
                            //
                            let slice = self.builder.build_load(ptr, "slice").into_struct_value();
                            let data_ptr = self.builder
                                .build_extract_value(slice, 0, "slice_data")
                                .unwrap()
                                .into_pointer_value();

                            let len = self.builder
                                .build_extract_value(slice, 1, "slice_len")
                                .unwrap()
                                .into_int_value();

                            ptr = data_ptr;
                            slice_len = Some(len);
                            current_ty = *inner.clone();
                        }

                        Type::REF(inner) | Type::CONST_REF(inner) | Type::POINTER(inner) | Type::CONST_POINTER(inner) => {
                            ptr = self.builder.build_load(ptr, "deref").into_pointer_value();
                            current_ty = *inner.clone();
                        }

                        _ => {
                            return Err(format!("ICE: cannot dereference type: {}", current_ty));
                        }
                    }
                }

                mir::ProjectionElem::Field(idx) => {
                    while let Type::REF(inner) | Type::CONST_REF(inner) | 
                        Type::POINTER(inner) | Type::CONST_POINTER(inner) = &current_ty
                    {
                        ptr = self.builder.build_load(ptr, "auto_deref").into_pointer_value();
                        current_ty = *inner.clone();
                    }

                    let struct_name = match &current_ty {
                        Type::STRUCT(name) => name.clone(),
                        _ => return Err(format!(
                            "ICE: field access on non-struct type {:?}", current_ty
                        )),
                    };

                    let _struct_ty = self.module.get_struct_type(&struct_name.symbol)
                        .ok_or_else(|| format!("ICE: struct type '{}' not found in module", struct_name))?;

                    ptr = self.builder.build_struct_gep(ptr, *idx as u32, "field_ptr")
                        .map_err(|_| format!("GEP failed: invalid field index {} on '{}'", idx, struct_name))?;

                    current_ty = self.hir_context
                        .find_struct_by_name(&struct_name.symbol)
                        .map(|def_id| {
                            let fields = self.hir_context.get_struct_fields(def_id);
                            fields[*idx].1.clone()
                        })
                        .unwrap_or(Type::VOID);

                    slice_len = None;
                }

                mir::ProjectionElem::Index(local_idx) => {
                    let idx_ptr = self.locals.get(local_idx)
                        .ok_or_else(|| format!("ICE: missing index local _{}", local_idx.0))?;

                    let idx_val = self.builder
                        .build_load(*idx_ptr, "idx_val")
                        .into_int_value();

                    let idx_ty = &mir_fn.locals[local_idx.0].ty;

                    let (len, element_ty, is_array) = match &current_ty {
                        Type::ARRAY(inner, size) => {
                            let usize_type = self.context.ptr_sized_int_type(&self.target_data, None);
                            let len = usize_type.const_int(*size as u64, false);

                            (len, inner.as_ref().clone(), true)
                        }

                        Type::SLICE(inner) => {
                            let len = slice_len.ok_or_else(|| {
                                "ICE: slice index did not have length metadata".to_string()
                            })?;

                            (len, inner.as_ref().clone(), false)
                        }

                        _ => {
                            return Err(format!(
                                "ICE: index projection on non-array/slice type {}",
                                current_ty
                            ));
                        }
                    };

                    let idx_val = self.emit_bounds_check(
                        idx_val,
                        idx_ty,
                        len,
                    )?;

                    if is_array {
                        let zero = self.context
                            .ptr_sized_int_type(&self.target_data, None)
                            .const_zero();

                        ptr = unsafe {
                            self.builder.build_gep(
                                ptr,
                                &[zero, idx_val],
                                "arr_idx",
                            )
                        };
                    } else {
                        ptr = unsafe {
                            self.builder.build_gep(
                                ptr,
                                &[idx_val],
                                "ptr_idx",
                            )
                        };
                    }

                    current_ty = element_ty;
                    slice_len = None;
                }
            }
        }

        Ok(ptr)
    }

    fn normalize_index(&self, value: IntValue<'c>, ty: &Type) -> IntValue<'c> {
        let usize_type = self.context.ptr_sized_int_type(&self.target_data, None);
        let source_width = value.get_type().get_bit_width();
        let target_width = usize_type.get_bit_width();

        if source_width == target_width {
            return value;
        }

        if source_width > target_width {
            return self.builder.build_int_truncate(value, usize_type, "index_trunc");
        }

        if matches!(ty, Type::I8 | Type::I16 | Type::I32 | Type::I64 | Type::ISIZE) {
            self.builder.build_int_s_extend(value, usize_type, "index_sext")
        } else {
            self.builder.build_int_z_extend(value, usize_type, "index_zext")
        }
    }

    fn emit_bounds_check(&self, index: IntValue<'c>, index_ty: &Type, len: IntValue<'c>) -> Result<IntValue<'c>, String> {
        let index = self.normalize_index(index, index_ty);

        let in_bounds = self.builder.build_int_compare(
            IntPredicate::ULT,
            index,
            len,
            "index_in_bounds",
        );

        let function = self.current_fn.ok_or_else(|| "ICE: bounds check emitted outside of a function".to_string())?;
        let success_bb = self.context.append_basic_block(function, "bounds_ok");
        let failure_bb = self.context.append_basic_block(function, "bounds_fail");

        self.builder.build_conditional_branch(
            in_bounds,
            success_bb,
            failure_bb,
        );

        self.builder.position_at_end(failure_bb);

        let void_type = self.context.void_type();
        let usize_type = self.context.ptr_sized_int_type(&self.target_data, None);

        let fail_fn = self.module.get_function("hydra_bounds_check_fail").unwrap_or_else(|| {
            let fn_type = void_type.fn_type(
                &[usize_type.into(), usize_type.into()],
                false,
            );

            self.module.add_function(
                "hydra_bounds_check_fail",
                fn_type,
                Some(inkwell::module::Linkage::External),
            )
        });

        self.builder.build_call(
            fail_fn,
            &[index.into(), len.into()],
            "bounds_fail",
        );

        self.builder.build_unreachable();
        self.builder.position_at_end(success_bb);

        Ok(index)
    }
}
