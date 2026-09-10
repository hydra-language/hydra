use super::super::Analyzer;

use errors::error::{
    HydraError,
    Span,
};
use ir::context::DefKind;
use ir::hir::{
    HIRExpr,
    HIRExprKind,
};
use ir::types::Type as IRType;
use parser::ast::Expr as ASTExpr;

impl<'ctx> Analyzer<'ctx> {


    pub(crate) fn lower_call_expr(&mut self, node: &ASTExpr, _expected: Option<&IRType>, span: Span) 
        -> Result<HIRExpr, HydraError> 
    {
        let ASTExpr::FunctionCall { callee, arguments, generic_args, .. } = node else {
            unreachable!();
        };

        match node {
            ASTExpr::FunctionCall { callee, arguments, generic_args, .. } => {
                let call_name_debug = match &**callee {
                    ASTExpr::Variable { name, .. } => name.lexeme.to_string(),
                    ASTExpr::Path { segments, .. } => segments.iter().map(|s| s.lexeme.as_str()).collect::<Vec<_>>().join("::"),
                    _ => "".to_string()
                };

                if call_name_debug == "print" || call_name_debug == "println" {
                    let mut args = Vec::new();
                    for arg in arguments { args.push(self.lower_expr(arg)?); }
                    return Ok(HIRExpr {
                        kind: HIRExprKind::BuiltinCall { name: call_name_debug, args },
                        ty: IRType::VOID,
                        span
                    });
                }

                // fetch the ID of the callee expression, not the outer FunctionCall
                let callee_id = crate::utils::get_expr_id(callee);
                let def_id = self.name_resolver.get_resolution(callee_id)
                    .ok_or_else(|| self.error("S002", format!("undefined function `{}`", call_name_debug), span))?;    

                let info = self.context.get_def(def_id).cloned().ok_or_else(|| 
                    {
                        self.error("S002", format!("missing definition for `{}`", call_name_debug), span)
                    }
                )?;

                // a path such as:
                //
                //     math::multiply(...)
                //
                // may have resolved to the local value `math`.
                //
                // in that case the final path segment is a method name,
                // not part of the receiver's definition path.
                if matches!(info.kind, DefKind::Variable { .. } | DefKind::Constant { .. }) {
                    if let ASTExpr::Path { segments, .. } = &**callee {
                        if segments.len() >= 2 {
                            let method_name = &segments.last().unwrap().lexeme;

                            // lower_expr(Path) is safe here:
                            //
                            // the resolver associated this path's NodeID
                            // with the receiver's DefID, so this produces
                            // VarRef(math), not a reference to multiply.
                            let lhs_expr = self.lower_expr_with_type(callee, None)?;
                            return self.lower_instance_method_call(
                                lhs_expr,
                                method_name,
                                arguments,
                                generic_args,
                                span,
                            );
                        }
                    }
                }

                let actual_def_id = if let DefKind::Struct { .. } = info.kind {
                    if let ASTExpr::Path { segments, .. } = &**callee {
                        let method_name = &segments.last().unwrap().lexeme;
                        let struct_name = info.absolute_path.join("::");

                        if let Some(type_methods) = self.impl_registry.get(&struct_name) {
                            if let Some(&m_def_id) = type_methods.get(method_name) {
                                m_def_id
                            } else {
                                return Err(self.error("S005", format!("struct `{}` has no associated function `{}`", struct_name, method_name), span));
                            }
                        } else {
                            return Err(self.error("S005", format!("struct `{}` has no associated function `{}`", struct_name, method_name), span));
                        }
                    } else {
                        return Err(self.error("S003", format!("target is a struct, not a function"), span));
                    }
                } else {
                    def_id
                };

                let actual_info = self.context.get_def(actual_def_id).unwrap();

                let (param_types, return_type, intrinsic) =
                match &actual_info.kind {
                    DefKind::Function {
                        params,
                        return_type,
                        intrinsic,
                        ..
                    } => (
                        params.clone(),
                        return_type.clone(),
                        *intrinsic,
                    ),

                    _ => {
                        return Err(self.error(
                            "S003",
                            "target is not a function",
                            span,
                        ));
                    }
                };

                let mut args = Vec::new();
                for (i, node) in arguments.iter().enumerate() {
                    let expected = param_types.get(i).and_then(|t| if matches!(t, IRType::GENERIC(_)) { None } else { Some(t) });
                    let mut arg = self.lower_expr_with_type(node, expected)?;
                    if let Some(target) = expected { arg = self.coerce_primitive(arg, target); }
                    args.push(arg);
                }

                let mut lowered_generics = Vec::new();
                for node in generic_args { lowered_generics.push(self.lower_type(node)?); }

                if let Some(kind) = intrinsic {
                    return Ok(HIRExpr {
                        kind: HIRExprKind::IntrinsicCall {
                            callee: actual_def_id,
                            kind,
                            args,
                            type_args: lowered_generics,
                        },
                        ty: return_type,
                        span,
                    });
                }

                Ok(HIRExpr { 
                    kind: HIRExprKind::Call {
                        callee: actual_def_id,
                        args,
                        generic_args: lowered_generics,
                    },
                    ty: return_type,
                    span,
                })
            }

            _ => unreachable!()
        }
    }
}
