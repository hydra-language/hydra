use lexer::{Token, TokenType};
use crate::ast::ASTNode;

pub struct Parser<'a> {
    tokens: Vec<Token<'a>>,
    current: usize,
}

impl<'a> Parser<'a> {
    pub fn new(tokens: Vec<Token<'a>>) -> Self {
        Self {
            tokens, current: 0
        }
    }

    pub fn parse(&mut self) -> Result<Vec<ASTNode<'a>>, String> {
        let mut statements = Vec::new();

        while !self.is_at_end() {
            statements.push(self.parse_declaration()?);
        }

        self.main_function_exists(&statements)?;

        Ok(statements)
    }

    fn parse_declaration(&mut self) -> Result<ASTNode<'a>, String> {
        if self.match_token(TokenType::Let) || self.match_token(TokenType::Const) {
            self.parse_variable()
        } else if self.match_token(TokenType::Function) {
            self.parse_function()
        } else if self.match_token(TokenType::Return) {
            self.parse_return()
        } else {
            self.parse_statement()
        }
    }

    fn parse_type(&mut self) -> Result<Token<'a>, String> {
        let var_type = &self.tokens[self.current].token_type;

        use TokenType::*;
        match var_type {
            Identifier(_) => Ok(self.advance().clone()),

            ISize | I8 | I16 | I32 | I64 | 
            USize | U8 | U16 | U32 | U64 |
            F32 | F64 | Char | Bool => Ok(self.advance().clone()),

            _ => Err("error: expected a type name".to_string()),
        }
    }

    fn parse_variable(&mut self) -> Result<ASTNode<'a>, String> {
        let is_const = self.previous().token_type == TokenType::Const;
        let name = self.consume(TokenType::Identifier("".to_string()), "error: expected variable name")?.clone();
        let mut type_annotation = None;
        
        if self.match_token(TokenType::Colon) {
            type_annotation = Some(self.parse_type()?);

        }
        self.consume(TokenType::Equal, "error: expected '=' after variable name")?;

        let initializer = self.parse_expression()?;
        self.consume(TokenType::Semicolon, "error: expected ';' at the end of line")?;

        Ok(ASTNode::VariableDeclaration {
            is_const,
            name,
            type_annotation,
            initializer: Box::new(initializer),
        })
    }

    fn parse_function(&mut self) -> Result<ASTNode<'a>, String> {
        let name = self.consume(TokenType::Identifier("".to_string()), "error: expected function name")?.clone();
        self.consume(TokenType::LeftParen, "error: expected '(' after function name")?;

        let mut parameters = Vec::new();
        if !self.check(TokenType::RightParen) {
            loop {
                let param_name = self.consume(TokenType::Identifier("".to_string()), "error: expected parameter name")?.clone();
                self.consume(TokenType::Colon, "error: expected ':' after parameter name")?;

                let param_type = self.parse_type()?;
                parameters.push((param_name.clone(), param_type.clone()));

                if !self.match_token(TokenType::Comma) {
                    break;
                }
            }
        }
        self.consume(TokenType::RightParen, "error: expected ')' after parameters")?;
        self.consume(TokenType::Arrow, "error: expected '->' after ')'")?;

        let return_type = self.parse_type()?.clone();
        self.consume(TokenType::LeftBrace, "error: expected '{' to open function body")?;

        let mut body = Vec::new();
        while !self.check(TokenType::RightBrace) {
            body.push(self.parse_declaration()?);
        }
        self.consume(TokenType::RightBrace, "error: expected '}' to close function body")?;

        Ok(ASTNode::FunctionDeclaration {
            name: name.clone(),
            parameters,
            return_type: return_type.clone(),
            body,
        })
    }

    fn parse_return(&mut self) -> Result<ASTNode<'a>, String> {
        let value = self.parse_expression()?;
        self.consume(TokenType::Semicolon, "error: expected ';' after return value")?;

        Ok(ASTNode::ReturnStatement {
            value: Box::new(value),
        })
    }

    fn parse_statement(&mut self) -> Result<ASTNode<'a>, String> {
        let expr = self.parse_expression()?;
        self.consume(TokenType::Semicolon, "error: expected ';' after expression")?;

        Ok(expr)
    }

    fn parse_expression(&mut self) -> Result<ASTNode<'a>, String> {
        let mut expr = self.parse_primary()?;
        if self.match_token(TokenType::LeftParen) {
            let name_token = match &expr {
                ASTNode::VariableExpression { name } => name.clone(),
                _ => return Err("error: expected function name before '('".to_string()),
            };

            expr = self.finish_parse_fn_call(name_token)?;
        }
        
        Ok(expr)
    }

    fn parse_primary(&mut self) -> Result<ASTNode<'a>, String> {
        let token_type = &self.tokens[self.current].token_type;

        use TokenType::*;
        match token_type {
            IntLiteral(_) | FloatLiteral(_) | StringLiteral(_) | CharLiteral(_) | BoolLiteral(_) => {
                self.advance();

                Ok(ASTNode::Expression { token: self.previous().clone() })
            }

            Identifier(_) => {
                self.advance();

                Ok(ASTNode::VariableExpression { name: self.previous().clone() })
            }

            LeftParen => {
                self.advance();

                let expr = self.parse_expression()?;
                self.consume(TokenType::RightParen, "error: expected ')' after expression");

                Ok(expr)
            }

            _ => {
                Err("error: expected primary expression".to_string())
            }
        }
    }

    fn finish_parse_fn_call(&mut self, name: Token<'a>) -> Result<ASTNode<'a>, String> {
        let mut args = Vec::new();

        if !self.check(TokenType::RightParen) {
            loop {
                args.push(self.parse_expression()?);

                if !self.match_token(TokenType::Comma) {
                    break;
                }
            }
        }
        self.consume(TokenType::RightParen, "error: expected ')' to close function body");

        Ok(ASTNode::FunctionCallExpression { name, arguments: args })
    }

    fn main_function_exists(&self, ast: &[ASTNode<'a>]) -> Result<(), String> {
        let main_found = ast.iter().any(|node| {
            if let ASTNode::FunctionDeclaration { name, parameters, return_type, .. } = node {
                name.lexeme == "main" && parameters.is_empty() && return_type.lexeme == "void"
            } else {
                false
            }
        });

        if main_found {
            Ok(())
        } else {
            Err("error: no 'main' function found\n
                help: your program must have an entry declared as\n\tfn main() -> void"
            .to_string())
        }
    }

    fn match_token(&mut self, token: TokenType) -> bool {
        if self.check(token) {
            self.advance();

            true
        } else {
            false
        }
    }

    fn consume(&mut self, token: TokenType, msg: &str) -> Result<&Token<'a>, String> {
        if self.check(token) {
            Ok(self.advance())
        } else {
            Err(msg.to_string())
        }
    }

    fn check(&self, token: TokenType) -> bool {
        if self.is_at_end() {
            return false;
        }

        use TokenType::*;
        match (&self.tokens[self.current].token_type, &token) {
            (Identifier(_), Identifier(_)) => true,
            (IntLiteral(_), IntLiteral(_)) => true,
            (FloatLiteral(_), FloatLiteral(_)) => true,
            (StringLiteral(_), StringLiteral(_)) => true,
            (a, b) => std::mem::discriminant(a) == std::mem::discriminant(b),
        }
    }

    fn advance(&mut self) -> &Token<'a> {
        if !self.is_at_end() {
            self.current += 1;
        }

        self.previous()
    }

    fn is_at_end(&self) -> bool {
        self.tokens[self.current].token_type == TokenType::EOF
    }

    fn previous(&self) -> &Token<'a> {
        &self.tokens[self.current - 1]
    }
}
