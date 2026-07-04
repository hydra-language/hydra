use super::Analyzer;
use std::mem;
use errors::error::Span;
use lexer::{Token, TokenType};
use parser::ast::ASTNode;
use crate::scope::Scope;
use ir::expr::BinaryOp;

impl Analyzer {

    pub(crate) fn enter_scope(&mut self) {
        let current_module_path = self.current_module.clone();
        let parent = mem::replace(&mut self.scope, Scope::new(current_module_path.clone()));
        
        let mut new_scope = Scope::new(current_module_path);
        new_scope.parent = Some(Box::new(parent));
        self.scope = new_scope;
    }

    pub(crate) fn leave_scope(&mut self) {
        let current_module_path = self.current_module.clone();
        let current_scope = mem::replace(&mut self.scope, Scope::new(current_module_path));
        
        if let Some(parent) = current_scope.parent() {
            self.scope = parent;
        }
    }

    pub(crate) fn get_token_from_node<'a>(&self, node: &ASTNode<'a>) -> Token<'a> {
        match node {
            ASTNode::VariableExpression { name } => name.clone(),
            ASTNode::Expression { token } | ASTNode::Primtive { token } => token.clone(),
            ASTNode::BinaryExpression { operator, .. } => operator.clone(),
            ASTNode::FunctionCallExpression { callee, .. } => self.get_token_from_node(callee), 
            ASTNode::PathExpression { segments } => segments.first().unwrap().clone(),
            ASTNode::VariableDeclaration { name, .. } => name.clone(),
            ASTNode::AssignmentExpression { operator, .. } => operator.clone(),
            ASTNode::MemberExpression { property, .. } => property.clone(),
            ASTNode::UnaryExpression { operator, .. } => operator.clone(),
            ASTNode::PostfixUnaryExpression { operator, .. } => operator.clone(),
            ASTNode::TypeIdentifier { type_token } => type_token.clone(),
            ASTNode::ReturnStatement { value } => self.get_token_from_node(value),

            ASTNode::BorrowExpression { right, .. } => self.get_token_from_node(right),
            ASTNode::DereferenceExpression { right } => self.get_token_from_node(right),
            ASTNode::StructInitializer { name, .. } => self.get_token_from_node(name),
            ASTNode::ArrayInitializer { token, .. } => token.clone(),
            ASTNode::ArrayAccess { array, .. } => self.get_token_from_node(array),
            ASTNode::MethodCallExpression { method, .. } => method.clone(),
            ASTNode::IfStatement { condition, .. } => self.get_token_from_node(condition),
            ASTNode::WhileLoop { condition, .. } => self.get_token_from_node(condition),
            ASTNode::ForLoop { variable, .. } => variable.clone(),
            ASTNode::ForEach { item, .. } => item.clone(),
            ASTNode::CastExpression { value, .. } => self.get_token_from_node(value),
            ASTNode::ExtensionDeclaration { target, .. } => self.get_token_from_node(target),
            ASTNode::FunctionDeclaration { name, .. } => name.clone(),
            ASTNode::StructDeclaration { name, .. } => name.clone(),
            ASTNode::Break { .. } | ASTNode::Continue { .. } => Token {
                token_type: TokenType::EOF,
                lexeme: "",
                span: Span::default(),
            },

            _ => Token {
                token_type: TokenType::EOF,
                lexeme: "",
                span: Span::default()
            }
        }
    }

    pub(crate) fn get_binary_op_from_token(&self, token: &TokenType) -> Option<BinaryOp> {
        match token {
            TokenType::PlusEqual => Some(BinaryOp::ADD),
            TokenType::MinusEqual => Some(BinaryOp::SUB),
            TokenType::StarEqual => Some(BinaryOp::MUL),
            TokenType::ForwardSlashEqual => Some(BinaryOp::DIV),
            TokenType::ModuloEqual => Some(BinaryOp::MOD),

            _ => None
        }
    }

}


