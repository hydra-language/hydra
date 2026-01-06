// TODO:
//      modularize for generating nodes (ie. separate files for function nodes, variable nodes, etc)

use lexer::{Token, TokenType};
use crate::{ParserError, ast::ASTNode};

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

    pub fn parse(&mut self) -> Result<Vec<ASTNode<'a>>, ParserError<'a>> {
        let mut statements = Vec::new();

        while !self.is_at_end() {
            statements.push(self.parse_declaration()?);
        }

        self.main_function_exists(&statements)?;

        Ok(statements)
    }

    fn parse_declaration(&mut self) -> Result<ASTNode<'a>, ParserError<'a>> {
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

    fn parse_type(&mut self) -> Result<Box<ASTNode<'a>>, ParserError<'a>> {
        // this is an array if true
        if self.match_token(TokenType::LeftBracket) {
            let start_token = self.previous().clone();

            let element_type = self.parse_type()?;

            self.consume(TokenType::Comma,"',' to separate type and array size")?;

            // simple expression for now
            // TODO: use parse_expression so computed lengths (ie. arr::length() * 2)
            // are able to go in the size
            let size_expr = self.parse_primary()?;
            self.consume(TokenType::RightBracket, "']' to close the array")?;

            Ok(Box::new(ASTNode::ArrayType {
                element_type,
                size: Box::new(size_expr),
                token: start_token,
            }))
        } else {
            // this is a simple variable assignment
            let current_token = &self.tokens[self.current];

            use TokenType::*;
            match &current_token.token_type {
                Identifier(_) |
                ISize | I8 | I16 | I32 | I64 | 
                USize | U8 | U16 | U32 | U64 |
                F32 | F64 | Char | Bool => {
                    let type_token = self.advance().clone();
                    Ok(Box::new(ASTNode::TypeIdentifier { type_token }))
                },

                _ => Err(ParserError::Generic { 
                    message: "expected a type name or array type".to_string(),
                    token: current_token.clone(),
                    help: Some(format!("consider adding a type annotation"))
                })
            }
        }
    }

    fn parse_variable(&mut self) -> Result<ASTNode<'a>, ParserError<'a>> {
        let is_const = self.previous().token_type == TokenType::Const;
        let name = self.consume(TokenType::Identifier("".to_string()), "variable name")?.clone();
        let mut type_annotation = None;
        
        if self.match_token(TokenType::Colon) {
            type_annotation = Some(self.parse_type()?);

        }
        self.consume(TokenType::Equal, "'=' after variable name")?;

        let initializer = self.parse_expression()?;
        self.consume(TokenType::Semicolon, "';' at the end of line")?;

        Ok(ASTNode::VariableDeclaration {
            is_const,
            name,
            type_annotation,
            initializer: Box::new(initializer),
        })
    }

    fn parse_function(&mut self) -> Result<ASTNode<'a>, ParserError<'a>> {
        let name = self.consume(TokenType::Identifier("".to_string()), "function name")?.clone();
        self.consume(TokenType::LeftParen, "'(' after function name")?;

        let mut parameters = Vec::new();
        if !self.check(TokenType::RightParen) {
            loop {
                let param_name = self.consume(TokenType::Identifier("".to_string()), "parameter name")?.clone();
                self.consume(TokenType::Colon, "':' after parameter name")?;

                let param_type = self.parse_type()?;
                parameters.push((param_name.clone(), param_type.clone()));

                if !self.match_token(TokenType::Comma) {
                    break;
                }
            }
        }
        self.consume(TokenType::RightParen, "')' after parameters")?;
        self.consume(TokenType::Arrow, "'->' after ')'")?;

        let return_type = self.parse_type()?.clone();
        self.consume(TokenType::LeftBrace, "'{' to open function body")?;

        let mut body = Vec::new();
        while !self.check(TokenType::RightBrace) {
            body.push(self.parse_declaration()?);
        }
        self.consume(TokenType::RightBrace, "'}' to close function body")?;

        Ok(ASTNode::FunctionDeclaration {
            name: name.clone(),
            parameters,
            return_type: return_type.clone(),
            body,
        })
    }

    fn parse_return(&mut self) -> Result<ASTNode<'a>, ParserError<'a>> {
        let value = self.parse_expression()?;
        self.consume(TokenType::Semicolon, "';' after return value")?;

        Ok(ASTNode::ReturnStatement {
            value: Box::new(value),
        })
    }

    fn parse_statement(&mut self) -> Result<ASTNode<'a>, ParserError<'a>> {
        let expr = self.parse_expression()?;
        self.consume(TokenType::Semicolon, "';' after expression")?;

        Ok(expr)
    }

    fn parse_expression(&mut self) -> Result<ASTNode<'a>, ParserError<'a>> {
        self.parse_assignment()
    }

    fn parse_assignment(&mut self) -> Result<ASTNode<'a>, ParserError<'a>> {
        // start with expresison that can be an assignment target
        let target = self.parse_additive()?; 

        if self.match_token(TokenType::Equal) ||
            self.match_token(TokenType::PlusEqual) ||
            self.match_token(TokenType::MinusEqual) ||
            self.match_token(TokenType::StarEqual) ||
            self.match_token(TokenType::ModuloEqual) ||
            self.match_token(TokenType::ForwardSlashEqual)
        {
            let operator = self.previous().clone();
            let value = self.parse_assignment()?;

            // target of assignment must be a variable expression (l-value)
            if let ASTNode::VariableExpression { name: _ } = &target {
                return Ok(ASTNode::AssignmentExpression {
                    target: Box::new(target),
                    operator,
                    value: Box::new(value)
                });
            } else {
                return Err(ParserError::Generic {
                    message: "invalid assignment target".to_string(),
                    token: self.previous().clone(),
                    help: None
                });
            }
        }

        // no assignment operator, return as is
        Ok(target)
    }

    fn parse_additive(&mut self) -> Result<ASTNode<'a>, ParserError<'a>> {
        let mut node = self.parse_multiplicative()?;

        loop {
            let operator = if self.match_token(TokenType::Plus) ||
                        self.match_token(TokenType::Minus) 
            {
                Some(self.previous().clone())
            } else {
                None
            };

            if let Some(op) = operator {
                let right = self.parse_call()?;

                node = ASTNode::BinaryExpression {
                    left: Box::new(node),
                    operator: op,
                    right: Box::new(right)
                };
            } else {
                break;
            }
        }

        Ok(node)
    }

    fn parse_multiplicative(&mut self) -> Result<ASTNode<'a>, ParserError<'a>> {
        let mut node = self.parse_call()?;

        loop {
            let operator = if self.match_token(TokenType::Star) || 
                        self.match_token(TokenType::ForwardSlash) || 
                        self.match_token(TokenType::Modulo) 
            {
                Some(self.previous().clone())    
            } else {
                None
            };

            if let Some(op) = operator {
                let right = self.parse_call()?;

                node = ASTNode::BinaryExpression {
                    left: Box::new(node),
                    operator: op,
                    right: Box::new(right),
                };
            } else {
                break;
            }
        }

        Ok(node)
    }

    fn parse_call(&mut self) -> Result<ASTNode<'a>, ParserError<'a>> {
        let mut expr = self.parse_primary()?;

        if self.match_token(TokenType::LeftParen) {
            let token_name = match &expr {
                ASTNode::VariableExpression { name } => name.clone(),

                _ => {
                    return Err(ParserError::Generic { 
                        message: "expected function name before '('".to_string(), 
                        token: self.previous().clone(),
                        help: Some(format!("consider naming the function"))
                    })
                }
            };
            
            expr = self.finish_parse_fn_call(token_name)?;
        }

        Ok(expr)
    }

    fn parse_primary(&mut self) -> Result<ASTNode<'a>, ParserError<'a>> {
        let current_token = &self.tokens[self.current];

        use TokenType::*;
        match &current_token.token_type {
            IntLiteral(_) | FloatLiteral(_) | StringLiteral(_) | CharLiteral(_)
            | BoolLiteral(_) => 
            {
                self.advance();
                Ok(ASTNode::Expression {
                    token: self.previous().clone(),
                })
            }

            Identifier(_) => {
                self.advance();
                Ok(ASTNode::VariableExpression {
                    name: self.previous().clone(),
                })
            }

            LeftParen => {
                self.advance();
                let expr = self.parse_expression()?; // Recurse to handle grouped expressions
                self.consume(TokenType::RightParen, "')' after expression")?;
                Ok(expr)
            }

            LeftBrace => {
                self.parse_array_initializer()
            }

            _ => Err(ParserError::Generic {
                message: "expected primary expression".to_string(),
                token: current_token.clone(),
                help: Some(format!("consider adding a primary expression"))
            }),
        }
    }

    fn parse_array_initializer(&mut self) -> Result<ASTNode<'a>, ParserError<'a>> {
        let start_token = self.consume(TokenType::LeftBrace, "'{' to start array initializer")?.clone();
        let mut elements = Vec::new();

        if !self.check(TokenType::RightBrace) {
            loop {
                elements.push(self.parse_expression()?);

                if !self.match_token(TokenType::Comma) {
                    break;
                }
            }
        }
        self.consume(TokenType::RightBrace, "'}' to close array initializer");

        Ok(ASTNode::ArrayInitializer {
            elements,
            token: start_token
        })
    }

    fn finish_parse_fn_call(&mut self, name: Token<'a>) -> Result<ASTNode<'a>, ParserError<'a>> {
        let mut args = Vec::new();

        if !self.check(TokenType::RightParen) {
            loop {
                args.push(self.parse_expression()?);

                if !self.match_token(TokenType::Comma) {
                    break;
                }
            }
        }
        self.consume(TokenType::RightParen, "')' to close function body")?;

        Ok(ASTNode::FunctionCallExpression { name, arguments: args })
    }

    fn main_function_exists(&self, ast: &[ASTNode<'a>]) -> Result<(), ParserError<'a>> {
        let main_found = ast.iter().any(|node| {
            if let ASTNode::FunctionDeclaration { name, parameters, return_type, .. } = node {
                let is_void_return = if let ASTNode::TypeIdentifier { type_token } = &**return_type {
                    type_token.lexeme == "void"
                } else {
                    false
                };
                name.lexeme == "main" && parameters.is_empty() && is_void_return
            } else {
                false
            }
        });

        if main_found {
            Ok(())
        } else {
            Err(ParserError::NoMainFunction)
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

    fn consume(&mut self, token: TokenType, expected: &str) -> Result<&Token<'a>, ParserError<'a>> {
        if self.check(token) {
            Ok(self.advance())
        } else {
            Err(ParserError::ExpectedToken {
                expected: expected.to_string(),
                found: self.tokens[self.current].clone(),
            })
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
