use std::collections::HashMap;

use super::super::Analyzer;

use errors::error::{HydraError, Span};

use ir::context::DefKind;
use ir::hir::{HIRExpr, HIRExprKind};
use ir::types::Type as IRType;

use parser::Type;
use parser::ast::Expr as ASTExpr;

impl<'ctx> Analyzer<'ctx> {

    pub(crate) fn lower_method_expr(&mut self, node: &ASTExpr, expected: Option<&IRType>, span: Span)
        -> Result<HIRExpr, HydraError> 
    {
        let ASTExpr::MethodCall { object, method, arguments, generic_args, .. } = node else {
            unreachable!();
        };

        let lhs_expr = self.lower_expr_with_type(object, None);
        self.lower_instance_method_call(lhs_expr?, &method.lexeme, arguments, generic_args, expected, span)
    }

    pub(crate) fn lower_instance_method_call(
        &mut self, lhs_expr: HIRExpr, method_name: &str, 
        arguments: &[ASTExpr], generic_args: &[Type], 
        expected: Option<&IRType>, span: Span
    ) -> Result<HIRExpr, HydraError> 
    {
        let actual_type = match &lhs_expr.ty {
            IRType::REF(inner) | IRType::CONST_REF(inner) | 
            IRType::POINTER(inner) | IRType::CONST_POINTER(inner) => inner.as_ref().clone(),

            _ => lhs_expr.ty.clone(),
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

        let (param_types, return_type, function_gp, owner_generic_count) = match &info.kind {
            DefKind::Function { params, return_type, generic_params, owner_generic_count, .. } => 
            {
                (params.clone(), return_type.clone(), generic_params.clone(), *owner_generic_count)
            }

            _ => {
                return Err(self.error(
                    "S003",
                    format!("'{}' is not a function", method_name),
                    span,
                ));
            }
        };

        let expected_self_ty = param_types.first().ok_or_else(|| {
            self.error(
                "S004",
                format!("method '{}' does not accept self", method_name),
                span,
            )
        })?;

        self.check_call_arity(
            &format!("method `{}`", method_name),
            param_types.len() - 1,
            arguments.len(),
            span,
        )?;

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

        if owner_generic_count > function_gp.len() {
            return Err(self.error("S006", "invalid generic metadata for method", span));
        }

        let callable_gp = &function_gp[owner_generic_count..];

        if generic_args.len() > callable_gp.len() {
            return Err(self.error(
                "S004",
                format!("method '{}' expected at most {} generic argument{}, found {}",
                    method_name,
                    callable_gp.len(),
                    if callable_gp.len() == 1 {
                        ""
                    } else {
                        "s"
                    },
                    generic_args.len(),
                ),
                span,
            ));
        }

        let mut substitutions = HashMap::<String, IRType>::new();

        Self::infer_generic_bindings(expected_self_ty, &self_arg.ty, &mut substitutions);
        for (name, node) in callable_gp.iter().zip(generic_args.iter()) {
            substitutions.insert(name.clone(), self.lower_type(node)?);
        }

        // self arg is always argument zero
        let mut args = vec![self_arg];

        for arg_node in arguments {
            let declared_param = param_types.get(args.len());
            let substituted_param = declared_param.map(|ty| ty.substitute(&substitutions));

            let argument_expected = substituted_param.as_ref().and_then(|ty| { 
                if ty.contains_generic() {
                    None
                } else {
                    Some(ty)
                }
            });

            let mut lowered_arg = self.lower_expr_with_type(arg_node, argument_expected)?;

            if let Some(target) = argument_expected {
                lowered_arg = self.coerce_primitive(lowered_arg, target);
            }

            if let Some(param_ty) = declared_param {
                Self::infer_generic_bindings(param_ty, &lowered_arg.ty, &mut substitutions);
            }

            args.push(lowered_arg);
        }

        if let Some(expected_ty) = expected {
            Self::infer_generic_bindings(&return_type, expected_ty, &mut substitutions);
        }

        let resolved_generic_args = function_gp.iter().map(|name| {
            substitutions.get(name).cloned().unwrap_or_else(|| {
                IRType::GENERIC(
                    name.clone()
                )
            })
        }).collect();

        let resolved_return_type = return_type.substitute(&substitutions);

        Ok(HIRExpr {
            kind: HIRExprKind::Call {
                callee: method_def_id,
                args,
                generic_args:
                resolved_generic_args,
            },

            ty: resolved_return_type,
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
                expected_base.def_id == actual_base.def_id && 
                expected_args.len() == actual_args.len() && 
                expected_args.iter().zip(actual_args).all(|(expected, actual)| {
                    self.receiver_type_matches(expected, actual)
                })
            }

            _ => expected == actual,
        }
    }
}
