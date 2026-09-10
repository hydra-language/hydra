use super::super::Analyzer;

use errors::error::{HydraError, Span};

use ir::context::DefKind;
use ir::hir::{HIRExpr, HIRExprKind};
use ir::types::Type as IRType;

use parser::Type;
use parser::ast::Expr as ASTExpr;

impl<'ctx> Analyzer<'ctx> {

    pub(crate) fn lower_method_expr(&mut self, node: &ASTExpr, _expected: Option<&IRType>, span: Span)
        -> Result<HIRExpr, HydraError> 
    {
        let ASTExpr::MethodCall { object, method, arguments, generic_args, .. } = node else {
            unreachable!();
        };

        let lhs_expr = self.lower_expr_with_type(object, None);
        self.lower_instance_method_call(lhs_expr?, &method.lexeme, arguments, generic_args, span)
    }

    pub(crate) fn lower_instance_method_call(
        &mut self, lhs_expr: HIRExpr, method_name: &str, 
        arguments: &[ASTExpr], generic_args: &[Type], span: Span
    ) -> Result<HIRExpr, HydraError> 
    {
        let actual_type = match &lhs_expr.ty {
            IRType::REF(inner) | IRType::CONST_REF(inner) | 
            IRType::POINTER(inner) | IRType::CONST_POINTER(inner) => inner.as_ref().clone(),

            _ => lhs_expr.ty.clone(),
        };

        let lookup_type = match &actual_type {
            IRType::GENERIC_INSTANCE(base, _) => *base.clone(),
            other => other.clone(),
        };

        let registry_key = self.get_impl_registry_key(&lhs_expr.ty);

        if registry_key.is_empty() {
            return Err(self.error(
                "S005",
                format!(
                    "type '{}' has no methods",
                    lhs_expr.ty
                ),
                span,
            ));
        }

        let method_def_id = {
            let type_methods = self
                .impl_registry
                .get(&registry_key)
                .ok_or_else(|| {
                    self.error(
                        "S005",
                        format!(
                            "type '{}' has no methods",
                            lhs_expr.ty
                        ),
                        span,
                    )
                })?;

            *type_methods
                .get(method_name)
                .ok_or_else(|| {
                    self.error(
                        "S005",
                        format!(
                            "method '{}' not found for type '{}'",
                            method_name,
                            lhs_expr.ty
                        ),
                        span,
                    )
                })?
        };

        let info = self
            .context
            .get_def(method_def_id)
            .ok_or_else(|| {
                self.error(
                    "S003",
                    format!("method '{}' has no definition", method_name),
                    span,
                )
            })?;

        let (param_types, return_type) = match &info.kind {
            DefKind::Function {
                params,
                return_type,
                ..
            } => (
                params.clone(),
                return_type.clone(),
            ),

            _ => {
                return Err(self.error(
                    "S003",
                    format!("'{}' is not a function", method_name),
                    span,
                ));
            }
        };

        let expected_self_ty = param_types
            .first()
            .ok_or_else(|| {
                self.error(
                    "S004",
                    format!("method '{}' does not accept self", method_name),
                    span,
                )
            })?;


        let self_arg = match (expected_self_ty, &lhs_expr.ty) {
            //
            // &mut T -> &T
            //
            // shared reborrow from a mutable reference.
            //
            (IRType::CONST_REF(expected_inner), IRType::REF(actual_inner)) 
            if self.receiver_type_matches(expected_inner, actual_inner) => {
                let pointee_ty = actual_inner.as_ref().clone();

                let deref = HIRExpr {
                    kind: HIRExprKind::Dereference {
                        target: Box::new(lhs_expr),
                    },
                    ty: pointee_ty.clone(),
                    span,
                };

                HIRExpr {
                    kind: HIRExprKind::Borrow {
                        is_mut: false,
                        target: Box::new(deref),
                    },
                    ty: IRType::CONST_REF(
                        Box::new(pointee_ty),
                    ),
                    span,
                }
            }

            //
            // already exactly compatible.
            //
            _ if self.receiver_type_matches(expected_self_ty, &lhs_expr.ty) => lhs_expr,

            //
            // T -> &mut T
            //
            (IRType::REF(_), _) => {
                let actual = lhs_expr.ty.clone();

                HIRExpr {
                    kind: HIRExprKind::Borrow {
                        is_mut: true,
                        target: Box::new(lhs_expr),
                    },
                    ty: IRType::REF(
                        Box::new(actual),
                    ),
                    span,
                }
            }

            //
            // T -> &T
            //
            (IRType::CONST_REF(_), _) => {
                let actual =
                lhs_expr.ty.clone();

                HIRExpr {
                    kind: HIRExprKind::Borrow {
                        is_mut: false,
                        target: Box::new(lhs_expr),
                    },
                    ty: IRType::CONST_REF(
                        Box::new(actual),
                    ),
                    span,
                }
            }

            _ => lhs_expr,
        };

        let expected_user_args = param_types.len().saturating_sub(1);

        if arguments.len() != expected_user_args {
            return Err(self.error(
                "S004",
                format!(
                    "method '{}' expected {} argument{}, found {}",
                    method_name,
                    expected_user_args,
                    if expected_user_args == 1 { "" } else { "s" },
                    arguments.len(),
                ),
                span,
            ));
        }

        let mut args = Vec::new();

        // self is always argument zero.
        args.push(self_arg);

        for arg_node in arguments {
            let expected_ty = param_types.get(args.len());

            let mut lowered_arg =
            self.lower_expr_with_type(
                arg_node,
                expected_ty,
            )?;

            if let Some(target) = expected_ty {
                lowered_arg =
                    self.coerce_primitive(
                        lowered_arg,
                        target,
                    );
            }

            args.push(lowered_arg);
        }

        let mut lowered_generics = Vec::new();

        for node in generic_args {
            lowered_generics.push(
                self.lower_type(node)?
            );
        }

        Ok(HIRExpr {
            kind: HIRExprKind::Call {
                callee: method_def_id,
                args,
                generic_args: lowered_generics,
            },
            ty: return_type,
            span,
        })
    }


    fn receiver_type_matches(&self, expected: &IRType, actual: &IRType) -> bool {
        match (expected, actual) {
            //
            // a generic can unify with any concrete type.
            //
            (IRType::GENERIC(_), _) => true,

            (IRType::REF(expected), IRType::REF(actual)) | (IRType::CONST_REF(expected), IRType::CONST_REF(actual)) | 
            (IRType::CONST_POINTER(expected), IRType::CONST_POINTER(actual)) | (IRType::SLICE(expected), IRType::SLICE(actual)) => 
            {
                self.receiver_type_matches(expected, actual)
            }

            (IRType::ARRAY(expected, expected_len), IRType::ARRAY(actual, actual_len)) => {
                expected_len == actual_len && self.receiver_type_matches(expected, actual)
            }

            (IRType::GENERIC_INSTANCE(expected_base, expected_args), IRType::GENERIC_INSTANCE(actual_base, actual_args)) => 
            {
                self.receiver_type_matches(expected_base, actual_base) && expected_args.len() == actual_args.len() && 
                expected_args.iter().zip(actual_args).all(|(expected, actual)| {
                    self.receiver_type_matches(expected, actual)
                })
            }

            _ => expected == actual,
        }
    }
}
