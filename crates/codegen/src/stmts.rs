use crate::CodeGen;
use ir::{context::DefKind, types::Type};
use mir::{Statement, StatementKind, Terminator, Place, MIRFunction, LocalID};
use inkwell::values::PointerValue;

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

                let Some(drop_def_id) = self.hir_context.get_drop_impl(type_name) else {
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
                let func_info = self.module.get_function(callee)
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

        for proj in &place.projection {
            match proj {
                mir::ProjectionElem::Deref => {
                    ptr = self.builder.build_load(ptr, "deref").into_pointer_value();

                    if let Type::REF(inner) | Type::CONST_REF(inner) | Type::POINTER(inner) = current_ty {
                        current_ty = *inner;
                    }
                }
                mir::ProjectionElem::Field(idx) => {
                    // Auto-deref through any pointer/reference wrappers before GEP.
                    // This handles the case where the MIR emits a Field projection
                    // directly on a &T or *T without an explicit preceding Deref.
                    while let Type::REF(inner) | Type::CONST_REF(inner) | Type::POINTER(inner) = &current_ty {
                        ptr = self.builder.build_load(ptr, "auto_deref").into_pointer_value();
                        current_ty = *inner.clone();
                    }

                    let struct_name = match &current_ty {
                        Type::STRUCT(name) => name.clone(),
                        _ => return Err(format!(
                            "ICE: field access on non-struct type {:?}", current_ty
                        )),
                    };

                    let _struct_ty = self.module.get_struct_type(&struct_name)
                        .ok_or_else(|| format!("ICE: struct type '{}' not found in module", struct_name))?;

                    ptr = self.builder.build_struct_gep(ptr, *idx as u32, "field_ptr")
                        .map_err(|_| format!("GEP failed: invalid field index {} on '{}'", idx, struct_name))?;

                    current_ty = self.hir_context
                        .find_struct_by_name(&struct_name)
                        .map(|def_id| {
                            let fields = self.hir_context.get_struct_fields(def_id);
                            fields[*idx].1.clone()  // (String name, Type, bool) → take the Type
                        })
                        .unwrap_or(Type::VOID);
                }
                mir::ProjectionElem::Index(local_idx) => {
                    let idx_ptr = self.locals.get(local_idx).unwrap();
                    let idx_val = self.builder.build_load(*idx_ptr, "idx_val").into_int_value();
                    
                    if let ir::types::Type::ARRAY(_, _) = current_ty {
                        let zero = self.context.i64_type().const_zero();
                        ptr = unsafe { self.builder.build_gep(ptr, &[zero, idx_val], "arr_idx") };
                    } else {
                        ptr = unsafe { self.builder.build_gep(ptr, &[idx_val], "ptr_idx") };
                    }
                }
            }
        }

        Ok(ptr)
    }
}
