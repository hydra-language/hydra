use parser::ast::{Expr as ASTExpr, Stmt as ASTStmt, Type as ASTType, NodeID};
use errors::error::Span;
use lexer::TokenType;
use ir::hir::HIRBinOp;

pub fn get_expr_span(expr: &ASTExpr) -> Span {
    match expr {
        ASTExpr::Literal { token, .. } => token.span,
        ASTExpr::Variable { name, .. } => name.span,
        ASTExpr::Path { segments, .. } => segments[0].span,
        ASTExpr::Binary { operator, .. } => operator.span,
        ASTExpr::Unary { operator, .. } => operator.span,
        ASTExpr::PostfixUnary { operator, .. } => operator.span,
        ASTExpr::Assignment { operator, .. } => operator.span,
        ASTExpr::MethodCall { method, .. } => method.span,
        ASTExpr::Member { property, .. } => property.span,
        ASTExpr::ArrayInitializer { token, .. } => token.span,
        ASTExpr::SliceInitializer { token, .. } => token.span,
        ASTExpr::ArrayAccess { token, .. } => token.span,
        ASTExpr::StructInitializer { name, .. } => get_expr_span(name),
        ASTExpr::If { condition, .. } => get_expr_span(condition),
        ASTExpr::While { condition, .. } => get_expr_span(condition),
        ASTExpr::For { variable, .. } => variable.span,
        ASTExpr::ForEach { item, .. } => item.span,
        ASTExpr::Cast { value, .. } => get_expr_span(value),
        ASTExpr::FunctionCall { callee, .. } => get_expr_span(callee),
        ASTExpr::Borrow { right, .. } => get_expr_span(right),
        ASTExpr::Dereference { right, .. } => get_expr_span(right),
    }
}

pub fn get_stmt_span(stmt: &ASTStmt) -> Span {
    match stmt {
        ASTStmt::VariableDecl { name, .. } => name.span,
        ASTStmt::Expr(expr) => get_expr_span(expr),
        ASTStmt::Return { value, .. } => value.as_ref().map(|e| get_expr_span(e)).unwrap_or_default(),
        ASTStmt::Break { condition, .. } => condition.as_ref().map(|e| get_expr_span(e)).unwrap_or_default(),
        ASTStmt::Continue { condition, .. } => condition.as_ref().map(|e| get_expr_span(e)).unwrap_or_default(),
    }
}

pub fn get_type_span(ty: &ASTType) -> Span {
    match ty {
        ASTType::Path { segments, .. } => segments[0].span,
        ASTType::Generic { base, .. } => get_type_span(base),
        ASTType::Borrow { inner, .. } => get_type_span(inner),
        ASTType::RawPointer { inner, .. } => get_type_span(inner),
        ASTType::Array { token, .. } => token.span,
        ASTType::Slice { token, .. } => token.span,
    }
}

pub fn get_binary_op_from_token(token: &TokenType) -> Option<HIRBinOp> {
    match token {
        TokenType::PlusEqual => Some(HIRBinOp::Add),
        TokenType::MinusEqual => Some(HIRBinOp::Sub),
        TokenType::StarEqual => Some(HIRBinOp::Mul),
        TokenType::ForwardSlashEqual => Some(HIRBinOp::Div),
        TokenType::ModuloEqual => Some(HIRBinOp::Mod),
        _ => None
    }
}

pub fn get_type_id(ty: &ASTType) -> NodeID {
    match ty {
        ASTType::Path { id, .. } => *id,
        ASTType::Generic { id, .. } => *id,
        ASTType::Borrow { id, .. } => *id,
        ASTType::RawPointer { id, .. } => *id,
        ASTType::Array { id, .. } => *id,
        ASTType::Slice { id, .. } => *id,
    }
}

pub fn get_expr_id(expr: &ASTExpr) -> NodeID {
    match expr {
        ASTExpr::Literal { id, .. } => *id,
        ASTExpr::Variable { id, .. } => *id,
        ASTExpr::Path { id, .. } => *id,
        ASTExpr::FunctionCall { id, .. } => *id,
        ASTExpr::MethodCall { id, .. } => *id,
        ASTExpr::Binary { id, .. } => *id,
        ASTExpr::Unary { id, .. } => *id,
        ASTExpr::PostfixUnary { id, .. } => *id,
        ASTExpr::Assignment { id, .. } => *id,
        ASTExpr::Borrow { id, .. } => *id,
        ASTExpr::Dereference { id, .. } => *id,
        ASTExpr::Member { id, .. } => *id,
        ASTExpr::ArrayInitializer { id, .. } => *id,
        ASTExpr::SliceInitializer { id, .. } => *id,
        ASTExpr::ArrayAccess { id, .. } => *id,
        ASTExpr::StructInitializer { id, .. } => *id,
        ASTExpr::Cast { id, .. } => *id,
        ASTExpr::If { id, .. } => *id,
        ASTExpr::While { id, .. } => *id,
        ASTExpr::For { id, .. } => *id,
        ASTExpr::ForEach { id, .. } => *id,
    }
}
