use super::Analyzer;
use std::mem;
use errors::{HydraError, generic::GenericError};
use lexer::{Token, TokenType};
use parser::ast::ASTNode;
use crate::scope::Scope;
use ir::expr::BinaryOp;

impl Analyzer {

    pub(crate) fn enter_scope(&mut self) {
        let parent = mem::replace(&mut self.scope, Scope::new());
        self.scope = Scope::new_child(parent);
    }

    pub(crate) fn leave_scope(&mut self) {
        let current_scope = mem::replace(&mut self.scope, Scope::new());
        let parent = current_scope.parent().expect("popped global scope");

        self.scope = parent;
    }

    pub(crate) fn dummy_token(&self) -> Token<'static> {
        Token { 
            token_type: TokenType::EOF, 
            lexeme: "", 
            line: 0, 
            column: 0 
        }
    }

    pub(crate) fn make_error(&self, msg: String, token: &Token) -> HydraError<'static> {
        HydraError::GENERIC(Box::new(GenericError {
            code: "E000", 
            message: msg, 
            help: None,
            token: Token { 
                token_type: token.token_type.clone(), 
                lexeme: "", 
                line: token.line, 
                column: token.column 
            }
        }))
    }

    pub(crate) fn make_generic_error(&self, msg: String) -> HydraError<'static> {
        HydraError::GENERIC(Box::new(GenericError { 
            code: "E000", 
            message: msg, 
            token: self.dummy_token(), 
            help: None 
        }))
    }

    pub(crate) fn get_token_from_node<'a>(&self, node: &ASTNode<'a>) -> Token<'a> {
        match node {
            ASTNode::VariableExpression { name } => name.clone(),
            ASTNode::Expression { token } | ASTNode::Primtive { token } => token.clone(),
            ASTNode::BinaryExpression { operator, .. } => operator.clone(),
            ASTNode::FunctionCallExpression { name, .. } => name.clone(),
            ASTNode::VariableDeclaration { name, .. } => name.clone(),
            ASTNode::AssignmentExpression { operator, .. } => operator.clone(),
            ASTNode::MemberExpression { property, .. } => property.clone(),
            ASTNode::UnaryExpression { operator, .. } => operator.clone(),
            ASTNode::PostfixUnaryExpression { operator, .. } => operator.clone(),
            ASTNode::TypeIdentifier { type_token } => type_token.clone(),
            ASTNode::ReturnStatement { value } => self.get_token_from_node(value),

            _ => self.dummy_token(),
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


