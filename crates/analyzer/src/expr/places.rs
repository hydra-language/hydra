use std::collections::HashMap;

use crate::{traits, utils};

use super::super::Analyzer;

use errors::error::{HydraError, Span};

use ir::context::DefKind;
use ir::hir::{HIRExpr, HIRExprKind};
use ir::types::Type as IRType;

use parser::ast::Expr as ASTExpr;

impl<'ctx> Analyzer<'ctx> {

    pub(crate) fn lower_place_expr(&mut self, node: &ASTExpr, _expected: Option<&IRType>, span: Span) 
        -> Result<HIRExpr, HydraError> 
    {
        match node {
            ASTExpr::Assignment { target, operator, value, .. } => {
                let lowered_target = self.lower_mutable_place_expr(target)?;

                self.check_assignable_place(&lowered_target)?;

                let mut lowered_value = self.lower_expr_with_type(
                    value,
                    Some(&lowered_target.ty)
                )?;

                lowered_value = self.coerce_primitive(lowered_value, &lowered_target.ty);

                let assign_value = if let Some(bin_op) = utils::get_binary_op_from_token(&operator.token_type)
                {
                    HIRExpr {
                        kind: HIRExprKind::Binary {
                            op: bin_op,
                            lhs: Box::new(lowered_target.clone()),
                            rhs: Box::new(lowered_value.clone()),
                        },
                        ty: lowered_target.ty.clone(),
                        span: operator.span,
                    }
                } else {
                    lowered_value
                };

                Ok(HIRExpr {
                    kind: HIRExprKind::Assign {
                        target: Box::new(lowered_target),
                        value: Box::new(assign_value),
                    },
                    ty: IRType::VOID,
                    span,
                })
            },

            ASTExpr::Member { object, property, .. } => {
                let lhs = self.lower_expr_with_type(object, None)?;
                self.lower_member_access(lhs, property, span)
            }

            ASTExpr::Borrow { is_mut, right, .. } => {
                let target_expr = if *is_mut {
                    self.lower_mutable_place_expr(right)?
                } else {
                    self.lower_expr(right)?
                };

                let ty = if *is_mut {
                    IRType::REF(Box::new(target_expr.ty.clone()))
                } else {
                    IRType::CONST_REF(Box::new(target_expr.ty.clone()))
                };

                Ok(HIRExpr {
                    kind: HIRExprKind::Borrow {
                        is_mut: *is_mut,
                        target: Box::new(target_expr),
                    },
                    ty,
                    span,
                })
            }

            ASTExpr::Dereference { right, .. } => {
                let target_expr = self.lower_expr(right)?;

                self.lower_dereference_expr(
                    target_expr,
                    false,
                    span,
                )
            }

            _ => unreachable!()
        }
    }

    fn lower_dereference_expr(&mut self, target_expr: HIRExpr, is_mut: bool, span: Span) -> Result<HIRExpr, HydraError> 
    {
        let native_inner_ty = match &target_expr.ty {
            IRType::REF(inner) | IRType::CONST_REF(inner) |
            IRType::POINTER(inner) | IRType::CONST_POINTER(inner) => {
                Some(*inner.clone())
            }

            _ => None,
        };

        if let Some(inner_ty) = native_inner_ty {
            return Ok(HIRExpr {
                kind: HIRExprKind::Dereference {
                    target: Box::new(target_expr),
                },
                ty: inner_ty,
                span,
            });
        }

        self.lower_deref_trait(
            target_expr,
            is_mut,
            span,
        )
    }

    fn lower_deref_trait(&mut self, target_expr: HIRExpr, is_mut: bool, span: Span) -> Result<HIRExpr, HydraError> 
    {
        let (trait_path, method_name) = if is_mut {
            (traits::DEREF_MUT_TRAIT_PATH, "deref_mut")
        } else {
            (traits::DEREF_TRAIT_PATH, "deref")
        };

        let trait_def_id = self.context.find_trait_by_path(trait_path).ok_or_else(|| {
            self.error(
                "S002",
                format!("compiler dereference support requires `{trait_path}`"),
                span,
            )
        })?;

        let matching_impls = self.context.trait_impls.iter().filter(|trait_impl| {
            trait_impl.trait_def_id == trait_def_id &&
            self.trait_solver().trait_impl_is_applicable(
                trait_impl,
                &target_expr.ty,
                &self.current_trait_bounds,
            )
        }).cloned().collect::<Vec<_>>();

        let trait_impl = match matching_impls.as_slice() {
            [] => {
                let message = if is_mut {
                    format!("cannot mutably dereference type `{}`", target_expr.ty)
                } else {
                    format!("cannot dereference type `{}`", target_expr.ty)
                };

                return Err(self.error(
                    "T004",
                    message,
                    span,
                ));
            }

            [trait_impl] => trait_impl,

            _ => {
                let message = if is_mut {
                    format!("mutable dereference target is ambiguous for type `{}`", target_expr.ty)
                } else {
                    format!("dereference target is ambiguous for type `{}`", target_expr.ty)
                };

                return Err(self.error(
                    "T004",
                    message,
                    span,
                ));
            }
        };

        let method_def_id = trait_impl.methods.get(method_name).copied().ok_or_else(|| {
            self.error(
                "S022",
                format!("trait implementation is missing method `{method_name}`"),
                span,
            )
        })?;

        let method_info = self.context.get_def(method_def_id).cloned().ok_or_else(|| {
            self.error(
                "S002",
                format!("missing definition for `{method_name}`"),
                span,
            )
        })?;

        let DefKind::Function { params, return_type, generic_params, trait_bounds, .. } = method_info.kind else 
        {
            return Err(self.error(
                "S022",
                format!("`{method_name}` is not a function"),
                span,
            ));
        };

        let expected_self_ty = params.first().ok_or_else(|| {
            self.error(
                "S022",
                format!("`{method_name}` does not accept `self`"),
                span,
            )
        })?;

        let receiver_ty = target_expr.ty.clone();

        if is_mut {
            self.check_assignable_place(&target_expr)?;
        }

        let self_arg_ty = if is_mut {
            IRType::REF(Box::new(receiver_ty))
        } else {
            IRType::CONST_REF(Box::new(receiver_ty))
        };

        let self_arg = HIRExpr {
            kind: HIRExprKind::Borrow {
                is_mut,
                target: Box::new(target_expr),
            },
            ty: self_arg_ty,
            span,
        };

        let mut substitutions = HashMap::new();

        Self::infer_generic_bindings(
            expected_self_ty,
            &self_arg.ty,
            &mut substitutions,
        );

        self.validate_trait_bounds(
            &trait_bounds,
            &substitutions,
            span,
        )?;

        let resolved_generic_args = generic_params.iter().map(|name| {
            substitutions.get(name).cloned().unwrap_or_else(|| IRType::GENERIC(name.clone()))
        }).collect::<Vec<_>>();

        let resolved_return_type = return_type.substitute(&substitutions);

        let inner_ty = if is_mut {
            match &resolved_return_type {
                IRType::REF(inner) => {
                    *inner.clone()
                }

                _ => {
                    return Err(self.error(
                        "S022",
                        "`DerefMut::deref_mut` must return a mutable reference",
                        span,
                    ));
                }
            }
        } else {
            match &resolved_return_type {
                IRType::CONST_REF(inner) => {
                    *inner.clone()
                }

                _ => {
                    return Err(self.error(
                        "S022",
                        "`Deref::deref` must return a shared reference",
                        span,
                    ));
                }
            }
        };

        let deref_call = HIRExpr {
            kind: HIRExprKind::Call {
                callee: self.hir_function_ref(method_def_id),
                args: vec![self_arg],
                generic_args: resolved_generic_args,
                trait_dispatch: None,
            },
            ty: resolved_return_type,
            span,
        };

        Ok(HIRExpr {
            kind: HIRExprKind::Dereference {
                target: Box::new(deref_call),
            },
            ty: inner_ty,
            span,
        })
    }

    fn lower_mutable_place_expr(&mut self, node: &ASTExpr) -> Result<HIRExpr, HydraError> {
        let span = utils::get_expr_span(node);

        match node {
            ASTExpr::Dereference { right, .. } => {
                let target_expr = self.lower_expr(right)?;
                self.lower_dereference_expr(target_expr, true, span)
            }

            ASTExpr::Member { object, property, .. } => {
                let lhs = self.lower_mutable_place_expr(object)?;
                self.lower_member_access(lhs, property, span)
            }

            ASTExpr::ArrayAccess { array, index, .. } => {
                let array = self.lower_mutable_place_expr(array)?;
                self.lower_array_access_from_base(array, index, span)
            }

            _ => self.lower_expr_with_type(node, None)
        }
    }

    fn lower_member_access(&mut self, lhs: HIRExpr, property: &lexer::Token, span: Span) -> Result<HIRExpr, HydraError> 
    {
        let actual_type = match &lhs.ty {
            IRType::REF(inner) |
            IRType::CONST_REF(inner) => inner.as_ref(),

            _ => &lhs.ty,
        };

        match actual_type {
            IRType::STRUCT(type_ref) | IRType::GENERIC_INSTANCE(type_ref, _) => {
                let Some(info) = self.context.get_def(type_ref.def_id) else {
                    return Err(self.error(
                        "S005",
                        format!(
                            "struct '{}' has no field '{}'",
                            type_ref,
                            property.lexeme,
                        ),
                        property.span,
                    ));
                };

                let DefKind::Struct { fields, .. } = &info.kind else {
                    return Err(self.error(
                        "S005",
                        format!(
                            "struct '{}' has no field '{}'",
                            type_ref,
                            property.lexeme,
                        ),
                        property.span,
                    ));
                };

                let Some(idx) = fields.iter().position(|(field_name, _, _)| field_name == &property.lexeme) else 
                {
                    return Err(self.error(
                        "S005",
                        format!(
                            "struct '{}' has no field '{}'",
                            type_ref,
                            property.lexeme,
                        ),
                        property.span,
                    ));
                };

                let (_, field_type, is_pub) = &fields[idx];

                if !self.can_access_struct_field(info, *is_pub) {
                    return Err(self.error(
                        "S019",
                        format!(
                            "field `{}` of struct `{}` is private",
                            property.lexeme,
                            type_ref,
                        ),
                        property.span,
                    ));
                }

                Ok(HIRExpr {
                    kind: HIRExprKind::FieldAccess {
                        object: Box::new(lhs),
                        field_index: idx,
                    },
                    ty: field_type.clone(),
                    span,
                })
            }

            IRType::ARRAY(_, size) if property.lexeme == "len" => {
                Ok(HIRExpr {
                    kind: HIRExprKind::IntLiteral(
                        *size as i64,
                    ),
                    ty: IRType::I32,
                    span,
                })
            }

            _ => Err(self.error(
                "S005",
                format!(
                    "'{}' has no property '{}'",
                    lhs.ty,
                    property.lexeme,
                ),
                property.span,
            ))
        }
    }

    fn check_assignable_place(&self, expr: &HIRExpr) -> Result<(), HydraError> {
        match &expr.kind {
            //
            // x = ...
            //
            HIRExprKind::VarRef(def_id) => {
                let info = self
                    .context
                    .get_def(*def_id)
                    .ok_or_else(|| {
                        self.error(
                            "S002",
                            "missing definition for assignment target",
                            expr.span,
                        )
                    })?;

                match &info.kind {
                    DefKind::Variable {
                        is_mutable: true,
                        ..
                    } => Ok(()),

                    DefKind::Variable {
                        is_mutable: false,
                        ..
                    }
                    | DefKind::Constant { .. } => {
                        Err(self.error(
                            "S009",
                            format!(
                                "cannot assign to immutable binding `{}`",
                                info.name
                            ),
                            expr.span,
                        ))
                    }

                    _ => Err(self.error(
                        "S009",
                        "invalid assignment target",
                        expr.span,
                    )),
                }
            }

            //
            // *p = ...
            //
            // IMPORTANT:
            //
            // do not recurse into `target` here.
            //
            //     const p: *mut i32 = ...;
            //     *p = 42;
            //
            // `p` itself cannot be rebound, but the memory reachable through
            // the *mut pointer is writable.
            //
            HIRExprKind::Dereference { target } => {
                match &target.ty {
                    IRType::POINTER(_) | IRType::REF(_) => Ok(()),

                    IRType::CONST_POINTER(_) => {
                        Err(self.error(
                            "S009",
                            "cannot assign through immutable pointer",
                            expr.span,
                        ))
                    }

                    IRType::CONST_REF(_) => {
                        Err(self.error(
                            "S009",
                            "cannot assign through immutable reference",
                            expr.span,
                        ))
                    }

                    _ => Err(self.error(
                        "S009",
                        "invalid assignment target",
                        expr.span,
                    )),
                }
            }

            //
            // foo.field = ...
            //
            HIRExprKind::FieldAccess { object, .. } => {
                self.check_projection_base_assignable(object)
            }

            //
            // array[index] = ...
            //
            HIRExprKind::ArrayAccess { array, .. } => {
                self.check_projection_base_assignable(array)
            }

            _ => Err(self.error(
                "S009",
                "invalid assignment target",
                expr.span,
            )),
        }
    }

    fn check_projection_base_assignable(&self, base: &HIRExpr) -> Result<(), HydraError> {
        match &base.ty {
            //
            // field access through a mutable ref/pointer is an implicit
            // dereference boundary:
            //
            //     const p: &mut Foo = ...;
            //     p.value = 42;
            //
            // the binding `p` is immutable, but its pointee is mutable.
            //
            IRType::REF(_) | IRType::POINTER(_) => Ok(()),

            IRType::CONST_REF(_) => {
                Err(self.error(
                    "S009",
                    "cannot assign through immutable reference",
                    base.span,
                ))
            }

            IRType::CONST_POINTER(_) => {
                Err(self.error(
                    "S009",
                    "cannot assign through immutable pointer",
                    base.span,
                ))
            }

            //
            // ordinary projection:
            //
            //     x.field
            //     x[index]
            //
            // inherits assignability from x.
            //
            _ => self.check_assignable_place(base),
        }
    }
}
