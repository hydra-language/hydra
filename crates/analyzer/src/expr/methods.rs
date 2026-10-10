use std::collections::HashMap;

use super::super::Analyzer;

use errors::error::{HydraError, Span};

use ir::context::{DefID, DefKind};
use ir::hir::{HIRExpr, HIRExprKind};
use ir::types::Type as IRType;

use parser::Type;
use parser::ast::Expr as ASTExpr;

#[derive(Debug, Clone, Copy)]
struct ResolvedMethod {
    method_def_id: DefID,
    trait_def_id: Option<DefID>,
    deferred_trait_dispatch: bool,
}

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
        &mut self, 
        lhs_expr: HIRExpr, 
        method_name: &str, 
        arguments: &[ASTExpr], 
        generic_args: &[Type], 
        expected: Option<&IRType>, 
        span: Span
    ) -> Result<HIRExpr, HydraError> 
    {
        if matches!(method_name, "write" | "writeln") && self.is_formatter_type(&lhs_expr.ty) {
            return self.lower_formatter_write(lhs_expr, arguments, method_name == "writeln", span);
        }

        let resolution = self.resolve_instance_method(&lhs_expr.ty, method_name, span)?;

        let method_def_id = resolution.method_def_id;

        let info = self.context.get_def(method_def_id).ok_or_else(|| {
            self.error(
                "S003",
                format!("method '{}' has no definition", method_name),
                span,
            )
        })?;

        let (param_types, return_type, function_gp, trait_bounds, owner_generic_count) = match &info.kind {
            DefKind::Function { params, return_type, generic_params,trait_bounds, owner_generic_count, .. } => 
            {
                (params.clone(), return_type.clone(), generic_params.clone(), trait_bounds, *owner_generic_count)
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
            // &&T -> &T
            // &mut &T -> &T
            //
            // method lookup may find a method on T through a reference to
            // an existing &T receiver. In that case we need to remove the
            // extra receiver reference, not add another one.
            //
            (IRType::CONST_REF(_), IRType::CONST_REF(actual_inner) | IRType::REF(actual_inner))
            if self.receiver_type_matches(expected_self_ty, actual_inner.as_ref()) =>
            {
                let ty = actual_inner.as_ref().clone();

                HIRExpr {
                    kind: HIRExprKind::Dereference {
                        target: Box::new(lhs_expr),
                    },
                    ty,
                    span,
                }
            }

            //
            // &mut T -> &T
            //
            (IRType::CONST_REF(expected_inner), IRType::REF(actual_inner))
            if self.receiver_type_matches(expected_inner, actual_inner) =>
            {
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
                    ty: IRType::CONST_REF(Box::new(pointee_ty)),
                    span,
                }
            }

            _ if self.receiver_type_matches(expected_self_ty, &lhs_expr.ty) => lhs_expr,

            (IRType::REF(_), _) => {
                let actual = lhs_expr.ty.clone();

                HIRExpr {
                    kind: HIRExprKind::Borrow {
                        is_mut: true,
                        target: Box::new(lhs_expr),
                    },
                    ty: IRType::REF(Box::new(actual)),
                    span,
                }
            }

            (IRType::CONST_REF(_), _) => {
                let actual = lhs_expr.ty.clone();

                HIRExpr {
                    kind: HIRExprKind::Borrow {
                        is_mut: false,
                        target: Box::new(lhs_expr),
                    },
                    ty: IRType::CONST_REF(Box::new(actual)),
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

        self.validate_trait_bounds(&trait_bounds, &substitutions, span)?;

        for (name, node) in callable_gp.iter().zip(generic_args.iter()) {
            substitutions.insert(name.clone(), self.lower_type(node)?);
        }

        // self arg is always argument zero
        let mut args = vec![self_arg];

        for arg_node in arguments {
            let declared_param = param_types.get(args.len());
            let substituted_param = declared_param.map(|ty| ty.substitute(&substitutions));

            let argument_expected = substituted_param.as_ref().filter(|&ty| !ty.contains_generic());

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
                trait_dispatch: if resolution.deferred_trait_dispatch {
                    resolution.trait_def_id
                } else {
                    None
                },
            },

            ty: resolved_return_type,
            span,
        })
    }

    fn resolve_instance_method(&self, receiver_ty: &IRType, method_name: &str, span: Span) -> Result<ResolvedMethod, HydraError> {
        let registry_key = self.get_impl_registry_key(receiver_ty);

        //
        // inherent methods always win.
        //
        if !registry_key.is_empty() {
            if let Some(method_def_id) = self.impl_registry.get(&registry_key).and_then(|methods| methods.get(method_name)).copied()
            {
                return Ok(ResolvedMethod {
                    method_def_id,
                    trait_def_id: None,
                    deferred_trait_dispatch: false,
                })
            }
        }

        //
        // references dispatch through their pointee, just like inherent
        // extension lookup already does
        //
        // raw pointers remain pointer types
        //
        let stripped_receiver = Self::strip_receiver_references(receiver_ty);
        let lookup_types = if stripped_receiver == receiver_ty {
            vec![receiver_ty]
        } else {
            vec![receiver_ty, stripped_receiver]
        };

        let matching_impls: Vec<_> = self.context.trait_impls.iter().filter(|trait_impl| {
            lookup_types.iter().any(|dispatch_ty| {
                self.context.trait_impl_is_applicable(trait_impl, dispatch_ty, &self.current_trait_bounds)
            })
        }).collect();

        let mut candidates = Vec::new();

        for trait_impl in &matching_impls {
            let Some(method_def_id) =
            trait_impl.methods.get(method_name).copied()
            else {
                continue;
            };

            if candidates.iter().any(|candidate: &ResolvedMethod| {
                candidate.trait_def_id == Some(trait_impl.trait_def_id)
            }) {
                continue;
            }

            candidates.push(ResolvedMethod {
                method_def_id,
                trait_def_id: Some(trait_impl.trait_def_id),
                deferred_trait_dispatch: false,
            });
        }

        for bound in &self.current_trait_bounds {
            if !lookup_types
                .iter()
                .any(|dispatch_ty| &bound.target_type == *dispatch_ty)
            {
                continue;
            }

            let Some(method_def_id) = self
                .context
                .get_trait_method(bound.trait_def_id, method_name)
            else {
                continue;
            };

            if candidates.iter().any(|candidate| {
                candidate.trait_def_id == Some(bound.trait_def_id)
            }) {
                continue;
            }

            candidates.push(ResolvedMethod {
                method_def_id,
                trait_def_id: Some(bound.trait_def_id),
                deferred_trait_dispatch: true,
            });
        }

        match candidates.as_slice() {
            [] => {
                let has_inherent_methods = !registry_key.is_empty() &&
                self.impl_registry.contains_key(&registry_key);

                if !has_inherent_methods && matching_impls.is_empty() {
                    return Err(self.error(
                        "S005",
                        format!("type '{}' has no methods", receiver_ty),
                        span,
                    ));
                }

                Err(self.error(
                    "S005",
                    format!(
                        "method '{}' not found for type '{}'",
                        method_name,
                        receiver_ty,
                    ),
                    span,
                ))
            }

            [candidate] => {
                if let Some(trait_def_id) = candidate.trait_def_id {
                    if self.is_drop_trait(trait_def_id) && method_name == "drop" {
                        return Err(self.error(
                            "S027",
                            "destructors cannot be called explicitly",
                            span,
                        ).with_help("values are destroyed automatically when they leave scope"));
                    }
                }
                
                Ok(*candidate)
            }

            _ => {
                let mut error = self.error(
                    "S024",
                    format!(
                        "method '{}' is ambiguous for type '{}'",
                        method_name,
                        receiver_ty,
                    ),
                    span,
                );

                for candidate in candidates {
                    let Some(trait_def_id) = candidate.trait_def_id else {
                        continue;
                    };

                    let Some(trait_info) = self.context.get_def(trait_def_id) else {
                        continue;
                    };

                    let trait_name = if trait_info.absolute_path.is_empty() {
                        trait_info.name.clone()
                    } else {
                        trait_info.absolute_path.join("::")
                    };

                    error = error.with_note(format!("candidate provided by trait `{trait_name}`"), trait_info.span);
                }

                Err(error)
            }
        }
    }

    fn strip_receiver_references(ty: &IRType) -> &IRType {
        match ty {
            IRType::REF(inner) | IRType::CONST_REF(inner) => {
                Self::strip_receiver_references(inner)
            }

            _ => ty,
        }
    }

    fn receiver_type_matches(&self, expected: &IRType, actual: &IRType) -> bool {
        match (expected, actual) {
            //
            // a generic can unify with any concrete type.
            //
            (IRType::GENERIC(_), _) => true,

            (IRType::REF(expected), IRType::REF(actual)) | (IRType::CONST_REF(expected), IRType::CONST_REF(actual)) |
            (IRType::POINTER(expected), IRType::POINTER(actual)) | (IRType::CONST_POINTER(expected), IRType::CONST_POINTER(actual)) |
            (IRType::SLICE(expected), IRType::SLICE(actual)) => 
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
