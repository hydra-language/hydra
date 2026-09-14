use errors::error::{HydraError, Span};

use lexer::TokenType;
use parser::ast::Expr as ASTExpr;

use ir::context::DefKind;
use ir::hir::{HIRExpr, HIRExprKind};
use ir::types::{Type as IRType, TypeRef};

use crate::Analyzer;

impl<'ctx> Analyzer<'ctx> {

    pub(crate) fn lower_value_expr(&mut self, node: &ASTExpr, expected: Option<&IRType>, span: Span) -> Result<HIRExpr, HydraError> {
        match node {
            ASTExpr::Literal { token, .. } => {
                match &token.token_type {
                    TokenType::IntLiteral(val) => {
                        let mut ty = IRType::I32;
                        if let Some(exp) = expected { ty = exp.clone(); }
                        Ok(HIRExpr { kind: HIRExprKind::IntLiteral(*val), ty, span })
                    },

                    TokenType::FloatLiteral(val) => {
                        let mut ty = IRType::F64; 
                        if let Some(exp) = expected {
                            if matches!(exp, IRType::F32 | IRType::F64) { ty = exp.clone(); }
                        }
                        Ok(HIRExpr { kind: HIRExprKind::FloatLiteral(*val), ty, span })
                    },

                    TokenType::StringLiteral(ref s) => Ok(HIRExpr {
                        kind: HIRExprKind::StringLiteral(s.clone()),
                        ty: IRType::CONST_REF(Box::new(
                            IRType::SLICE(Box::new(IRType::U8))
                        )),
                        span,
                    }),

                    TokenType::CharLiteral(c) => Ok(HIRExpr { kind: HIRExprKind::CharLiteral(*c), ty: IRType::CHAR, span }),
                    TokenType::BoolLiteral(b) => Ok(HIRExpr { kind: HIRExprKind::BoolLiteral(*b), ty: IRType::BOOL, span }),
                    _ => Err(self.error("S003", format!("unexpected literal: {:?}", token.token_type), token.span))
                }
            },

            ASTExpr::Variable { id, name } => {
                let def_id = self.name_resolver.get_resolution(*id)
                    .ok_or_else(|| self.error("S002", format!("undefined variable `{}`", name.lexeme), name.span))?;
                
                let info = self.context.get_def(def_id).unwrap();

                let ty = match &info.kind {
                    DefKind::Variable { ty, .. } | DefKind::Constant { ty, .. } | DefKind::Function { return_type: ty, .. } => ty.clone(),
                    _ => return Err(self.error("S003", format!("`{}` cannot be used as a value", name.lexeme), name.span))
                };

                Ok(HIRExpr { kind: HIRExprKind::VarRef(def_id), ty, span })
            },

            ASTExpr::Path { id, segments } => {
                let def_id = self.name_resolver.get_resolution(*id)
                    .ok_or_else(|| self.error("S002", format!("undefined path `{}`", segments[0].lexeme), span))?;
                
                let info = self.context.get_def(def_id).unwrap();
                let ty = match &info.kind {
                    DefKind::Variable { ty, .. } | DefKind::Constant { ty, .. } | 
                    DefKind::Function { return_type: ty, .. } => {
                        ty.clone()
                    }

                    DefKind::Struct { .. } => {
                        let symbol = if info.absolute_path.is_empty() {
                            info.name.clone()
                        } else {
                            info.absolute_path.join("::")
                        };

                        IRType::STRUCT(TypeRef::new(def_id, symbol))
                    }

                    _ => return Err(self.error("S003", format!("`{}` cannot be used as a value", segments[0].lexeme), span))
                };

                Ok(HIRExpr { kind: HIRExprKind::VarRef(def_id), ty, span })
            },

            _ => unreachable!("lower_value_expr() called with non-value expression")
        }
    }
}
