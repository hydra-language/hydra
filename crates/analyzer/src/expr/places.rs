use crate::utils;

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
                let lowered_target = self.lower_expr_with_type(target, None)?;
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
                        target: Box::new(lowered_target.clone()),
                        value: Box::new(assign_value),
                    },
                    ty: lowered_target.ty,
                    span,
                })
            },

            ASTExpr::Member { object, property, .. } => {
                let lhs = self.lower_expr_with_type(object, None)?;

                let actual_type = match &lhs.ty {
                    IRType::REF(inner) | IRType::CONST_REF(inner) => inner.as_ref(),
                    _ => &lhs.ty
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
                        Ok( HIRExpr {
                            kind:
                            HIRExprKind::IntLiteral(
                                *size as i64
                            ),
                            ty: IRType::I32,
                            span,
                        })
                    }

                    _ => {
                        Err(self.error(
                            "S005",
                            format!("'{}' has no property '{}'", lhs.ty, property.lexeme),
                            property.span,
                        ))
                    }
                }
            },


            ASTExpr::Borrow { is_mut, right, .. } => {
                let target_expr = self.lower_expr(right)?;
                let ty = if *is_mut { 
                    IRType::REF(Box::new(target_expr.ty.clone())) 
                } else { 
                    IRType::CONST_REF(Box::new(target_expr.ty.clone())) 
                };

                Ok(HIRExpr {
                    kind: HIRExprKind::Borrow { is_mut: *is_mut, target: Box::new(target_expr) },
                    ty,
                    span, 
                })
            }

            ASTExpr::Dereference { right, .. } => {
                let target_expr = self.lower_expr(right)?;

                let inner_ty = match &target_expr.ty {
                    IRType::REF(inner) | IRType::CONST_REF(inner) | 
                    IRType::POINTER(inner) | IRType::CONST_POINTER(inner) => *inner.clone(),
                    _ => return Err(self.error("T004", format!("cannot dereference type `{}`", target_expr.ty), span)),
                };

                Ok(HIRExpr {
                    kind: HIRExprKind::Dereference { target: Box::new(target_expr) },
                    ty: inner_ty,
                    span,
                })
            }

            _ => unreachable!()
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
