use errors::error::{HydraError, Span};

use ir::{
    context::{DefID, DefKind, SymbolInfo}, hir::{HIRBlock, HIRExpr, HIRExprKind, HIRStmt}, types::Type as IRType,
};

use parser::ast::Expr as ASTExpr;

use crate::{
    traits,
    Analyzer,
};

const FORMATTER_PATH: &str = "core::fmt::formatter::Formatter";
const FORMATTER_WRITE_PATH: &str = "core::fmt::formatter::Formatter::write";

impl<'ctx> Analyzer<'ctx> {

    pub(crate) fn is_formatter_type(&self, ty: &IRType) -> bool {
        match ty {
            IRType::REF(inner) | IRType::CONST_REF(inner) => {
                self.is_formatter_type(inner)
            }

            IRType::STRUCT(type_ref) | IRType::GENERIC_INSTANCE(type_ref, _) => {
                type_ref.symbol == FORMATTER_PATH
            }

            _ => false,
        }
    }

    pub(crate) fn lower_formatter_write(&mut self, formatter: HIRExpr, arguments: &[ASTExpr], span: Span) -> Result<HIRExpr, HydraError> 
    {
        let Some(format_node) = arguments.first() else {
            return Err(self.error(
                "S018",
                "method `write` expected at least 1 argument, found 0",
                span,
            ));
        };

        let format_expr = self.lower_expr(format_node)?;

        let format_string = match &format_expr.kind {
            HIRExprKind::StringLiteral(value) => {
                Some(value.clone())
            }

            _ => None,
        };

        //
        // Formatter::write(text)
        //
        // remains an ordinary runtime text write when `text` is not a
        // compile-time string literal
        //
        // it only becomes compiler-assisted formatting when there are
        // formatting arguments or a literal contains `{}`
        //
        let should_expand = arguments.len() > 1 || format_string.as_ref().is_some_and(|format| format.contains("{}"));

        if !should_expand {
            return self.make_formatter_write_call(
                formatter,
                format_expr,
                span,
            );
        }

        let Some(format_string) = format_string else {
            return Err(self.error(
                "S029",
                "format string must be a string literal when formatting arguments are provided",
                format_expr.span,
            ));
        };

        let parts = format_string.split("{}").map(str::to_string).collect::<Vec<_>>();
        let placeholder_count = parts.len() - 1;
        let formatting_arg_count = arguments.len() - 1;

        if placeholder_count != formatting_arg_count {
            return Err(self.error(
                "S018",
                format!(
                    "format string expected {} argument{}, found {}",
                    placeholder_count,
                    if placeholder_count == 1 { "" } else { "s" },
                    formatting_arg_count,
                ),
                span,
            ));
        }

        let display_trait_def_id = self.context.find_trait_by_path(traits::DISPLAY_TRAIT_PATH)
            .ok_or_else(|| {
                self.error(
                    "S002",
                    "compiler formatting support requires `core::fmt::Display`",
                    span,
                )
            })?;

        let mut stmts = Vec::new();
        let mut formatting_args = Vec::new();

        //
        // evaluate every formatting argument before performing any output
        //
        // this preserves ordinary call evaluation order:
        //
        //     formatter::write("{} {}", foo(), bar());
        //
        // evaluates foo() and bar() before the first byte is written.
        //
        // store shared references rather than moving the values because
        // Display::fmt receives `&self`
        //
        for (index, argument_node) in arguments.iter().skip(1).enumerate() {
            let argument = self.lower_expr(argument_node)?;
            let argument_ty = argument.ty.clone();

            if !self.context.type_implements_trait(&argument_ty, display_trait_def_id, &self.current_trait_bounds) 
            {
                return Err(self.error(
                    "S028",
                    format!(
                        "type `{}` does not implement `Display`",
                        argument_ty,
                    ),
                    argument.span,
                ).with_help(format!("implement `core::fmt::Display` for `{}`", argument_ty)),
                );
            }

            let reference_ty = IRType::CONST_REF(Box::new(argument_ty.clone()));
            let temp_name = format!("_format_arg_{}_{}", span.line, index);

            let temp_def_id = self.context.insert_def(
                SymbolInfo {
                    name: temp_name.clone(),
                    span: argument.span,
                    absolute_path: vec![temp_name],
                    kind: DefKind::Variable {
                        ty: reference_ty.clone(),
                        is_mutable: false,
                    },
                    is_pub: false,
                },
            );

            stmts.push(HIRStmt::VarDecl {
                def_id: temp_def_id,
                ty: reference_ty.clone(),
                init: Some(HIRExpr {
                    kind: HIRExprKind::Borrow {
                        is_mut: false,
                        target: Box::new(argument),
                    },
                    ty: reference_ty.clone(),
                    span,
                }),
                has_type_annotation: false,
                span,
            });

            formatting_args.push((
                argument_ty,
                HIRExpr {
                    kind: HIRExprKind::VarRef(
                        temp_def_id,
                    ),
                    ty: reference_ty,
                    span,
                },
            ));
        }

        for (index, part) in parts.iter().enumerate() {
            if !part.is_empty() {
                stmts.push(HIRStmt::Expr(
                    self.make_formatter_write_call(
                        formatter.clone(),
                        Self::format_string_literal(
                            part.clone(),
                            span,
                        ),
                        span,
                    )?,
                ));
            }

            if index >= placeholder_count {
                continue;
            }

            let (argument_ty, self_arg) = &formatting_args[index];

            stmts.push(HIRStmt::Expr(
                self.make_display_fmt_call(
                    display_trait_def_id,
                    argument_ty,
                    self_arg.clone(),
                    formatter.clone(),
                    span,
                )?,
            ));
        }

        Ok(HIRExpr {
            kind: HIRExprKind::Block(
                HIRBlock {
                    stmts,
                    span,
                },
            ),
            ty: IRType::VOID,
            span,
        })
    }

    fn make_formatter_write_call(&self, formatter: HIRExpr, text: HIRExpr, span: Span) -> Result<HIRExpr, HydraError> 
    {
        let write_def_id = self.context.find_function_by_name(FORMATTER_WRITE_PATH)
            .ok_or_else(|| {
                self.error(
                    "S002",
                    "missing `Formatter::write`",
                    span,
                )
            })?;

        let formatter_arg = self.formatter_mut_arg(formatter, span)?;

        Ok(HIRExpr {
            kind: HIRExprKind::Call {
                callee: write_def_id,
                args: vec![
                    formatter_arg,
                    text,
                ],
                generic_args: vec![],
                trait_dispatch: None,
            },

            ty: IRType::VOID,
            span,
        })
    }

    fn make_display_fmt_call(&self, display_trait_def_id: DefID, value_ty: &IRType, self_arg: HIRExpr, formatter: HIRExpr, span: Span) 
        -> Result<HIRExpr, HydraError> 
    {
        let concrete_method = self.context.trait_impls.iter().find_map(|trait_impl| {
            if trait_impl.trait_def_id != display_trait_def_id {
                return None;
            }

            if !self.context.trait_impl_is_applicable(trait_impl, value_ty, &self.current_trait_bounds) 
            {
                return None;
            }

            trait_impl.methods.get("fmt").copied()
        });

        let (callee, trait_dispatch) =
            if let Some(method_def_id) = concrete_method {
                (method_def_id, None)
            } else {
                let method_def_id = self.context.get_trait_method(display_trait_def_id, "fmt").ok_or_else(|| {
                    self.error(
                        "S002",
                        "`Display` is missing required method `fmt`",
                        span,
                    )
                })?;

                //
                // the trait requirement may only be known from:
                //
                //     where T: Display
                //
                // in that case there is intentionally no concrete method
                // yet
                //  
                // monomorphization resolves this call once T is known
                //
                (method_def_id, Some(display_trait_def_id))
            };

        let formatter_arg = self.formatter_mut_arg(formatter, span)?;

        Ok(HIRExpr {
            kind: HIRExprKind::Call {
                callee,
                args: vec![
                    self_arg,
                    formatter_arg,
                ],
                generic_args: vec![],
                trait_dispatch,
            },

            ty: IRType::VOID,
            span,
        })
    }

    fn formatter_mut_arg(&self, formatter: HIRExpr, span: Span) -> Result<HIRExpr, HydraError> {
        match &formatter.ty {
            IRType::REF(inner) if self.is_formatter_type(inner) => {
                Ok(formatter)
            }

            IRType::CONST_REF(inner) if self.is_formatter_type(inner) => {
                Err(self.error(
                    "S009",
                    "cannot write through an immutable `Formatter` reference",
                    span,
                ))
            }

            _ => {
                let formatter_ty = formatter.ty.clone();

                Ok(HIRExpr {
                    kind: HIRExprKind::Borrow {
                        is_mut: true,
                        target: Box::new(formatter),
                    },

                    ty: IRType::REF(
                        Box::new(formatter_ty),
                    ),

                    span,
                })
            }
        }
    }

    fn format_string_literal(value: String, span: Span) -> HIRExpr {
        HIRExpr {
            kind: HIRExprKind::StringLiteral(value),

            ty: IRType::CONST_REF(
                Box::new(
                    IRType::SLICE(
                        Box::new(IRType::U8),
                    ),
                ),
            ),

            span,
        }
    }
}
