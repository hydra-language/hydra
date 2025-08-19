/**
 * =========
 * parser.rs
 * =========
 * 
 * This file is responsible for parsing the Hydra source tokens
 */

use lexer::{Token, TokenType};
use crate::ast::*;

 pub struct Parser<'a> {
    tokens: Vec<Token<'a>>,
    current: usize,
 }

 impl<'a> Parser<'a> {
    pub fn new(tokens: Vec<Token<'a>>) -> Self {
        return Self {
            tokens,
            current: 0
        }
    }

    // Look at current token
    fn peek(&self) -> &TokenType {
        return &self.tokens[self.current].token_type
    }

    // Check if EOF
    fn is_at_end(&self) -> bool {
        return *self.peek() == TokenType::Eof
    }

    // Consume current token and return it
    fn advance(&mut self) -> &TokenType {
        if !self.is_at_end() {
            self.current += 1;
        }
        
        return &self.tokens[self.current - 1].token_type
    }

    // Check if current token matches given type
    fn match_token(&mut self, token_type: TokenType) -> bool {
        if *self.peek() == token_type {
            self.advance();
            
            return true
        } else {
            return false
        }
    }

    pub fn parse(&mut self) -> Result<Program, String> {
        let mut statements = Vec::new();

        while !self.is_at_end() {
            statements.push(self.parse_declaration()?);
        }
        
        return Ok(statements)
    }

    fn parse_declaration(&mut self) -> Result<Statement, String> {
        match self.peek() {
            TokenType::Let | TokenType::Const => self.parse_variable(),
            TokenType::Function => self.parse_function(),
            TokenType::Struct => self.parse_struct(),
            
            _ => self.parse_expr(),
        }
    }

    pub fn parse_variable(&self) -> Result<Statement, String> {

    }

    pub fn parse_function(&self) -> Result<Statement, String> {

    }

    pub fn parse_struct(&self) -> Result<Statement, String> {

    }

    pub fn parse_expr(&self) -> Result<Statement, String> {

    }

    pub fn parse_extension(&self) -> Result<Statement, String> {

    }
 }