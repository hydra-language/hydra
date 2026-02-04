use lexer::{Token, TokenType};
use crate::{ASTNode, ParserError};
use errors::{generic::GenericError, expected_found::ExpectedFoundError};

pub struct Parser<'a> {
    tokens: Vec<Token<'a>>,
    current: usize,
    errors: Vec<ParserError<'a>>
}

impl<'a> Parser<'a> {

    // ========================================================================
    // 1. LIFECYCLE & ENTRY POINT
    // ========================================================================

    pub fn new(tokens: Vec<Token<'a>>) -> Self {
        Self {
            tokens, 
            current: 0,
            errors: Vec::new(),
        }
    }

    pub fn parse(&mut self) -> Result<Vec<ASTNode<'a>>, Vec<ParserError<'a>>> {
        let mut statements = Vec::new();

        while !self.is_at_end() {
            match self.parse_declaration() {
                Ok(stmt) => statements.push(stmt),
                Err(e) => {
                    self.errors.push(e);
                    self.synchronize();
                }
            }
        }

        if self.errors.is_empty() {
            Ok(statements)
        } else {
            Err(self.errors.clone())
        }
    }

    // ========================================================================
    // 2. ERROR SYNCHRONIZATION
    // ========================================================================
    fn synchronize(&mut self) {
        self.advance();

        while !self.is_at_end() {
            if self.previous().token_type == TokenType::Semicolon {
                return;
            }

            // If we see a keyword that starts a statement, we assume we are back on track
            match self.peek().token_type {
                TokenType::Function | TokenType::Let | TokenType::Const | 
                TokenType::For | TokenType::If | TokenType::While | TokenType::Return => return,
                
                _ => {}
            }

            self.advance();
        }
    }

    // ========================================================================
    // 3. DECLARATION DISPATCHER
    // ========================================================================

    fn parse_declaration(&mut self) -> Result<ASTNode<'a>, ParserError<'a>> {
        if self.match_token(TokenType::Let) || self.match_token(TokenType::Const) {
            self.parse_variable()
        } else if self.match_token(TokenType::Function) {
            self.parse_function()
        } else if self.match_token(TokenType::Return) {
            self.parse_return()
        } else if self.match_token(TokenType::If) {
            self.parse_if()
        } else if self.match_token(TokenType::For) {
            self.parse_for()
        } else if self.match_token(TokenType::While) {
            self.parse_while()
        } else if self.match_token(TokenType::Break) {
            self.parse_break()
        } else if self.match_token(TokenType::Continue) {
            self.parse_continue()
        } else {
            self.parse_statement()
        }
    }

    // ========================================================================
    // 4. STATEMENTS & DECLARATIONS (Ordered: Basic -> Complex)
    // ========================================================================

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

    fn parse_return(&mut self) -> Result<ASTNode<'a>, ParserError<'a>> {
        let value = self.parse_expression()?;
        self.consume(TokenType::Semicolon, "';' after return value")?;

        Ok(ASTNode::ReturnStatement {
            value: Box::new(value),
        })
    }

    fn parse_break(&mut self) -> Result<ASTNode<'a>, ParserError<'a>> {
        let mut condition = None;

        if self.match_token(TokenType::If) {
            self.consume(TokenType::LeftParen, "'(' after 'break if'")?;
            condition = Some(Box::new(self.parse_expression()?));
            self.consume(TokenType::RightParen, "')' after condition")?;
        }

        self.consume(TokenType::Semicolon, "';' after break")?;

        Ok(ASTNode::Break { condition })
    }

    fn parse_continue(&mut self) -> Result<ASTNode<'a>, ParserError<'a>> {
        let mut condition = None;

        if self.match_token(TokenType::If) {
            self.consume(TokenType::LeftParen, "'(' after 'continue if'")?;
            condition = Some(Box::new(self.parse_expression()?));
            self.consume(TokenType::RightParen, "')' after condition")?;
        }

        self.consume(TokenType::Semicolon, "';' after continue")?;

        Ok(ASTNode::Continue { condition })
    }

    fn parse_statement(&mut self) -> Result<ASTNode<'a>, ParserError<'a>> {
        let expr = self.parse_expression()?;
        self.consume(TokenType::Semicolon, "';' after expression")?;

        Ok(expr)
    }

    fn parse_block(&mut self) -> Vec<ASTNode<'a>> {
        if let Err(e) = self.consume(TokenType::LeftBrace, "'{' to start block") {
            self.errors.push(e);

            return Vec::new();
        }

        let mut stmts = Vec::new();

        while !self.check(TokenType::RightBrace) && !self.is_at_end() {
            match self.parse_declaration() {
                Ok(stmt) => stmts.push(stmt),
                Err(e) => {
                    self.errors.push(e);
                    self.synchronize();
                }
            }
        }

        if let Err(e) = self.consume(TokenType::RightBrace, "'}' to end block") {
            self.errors.push(e);
        }

        stmts
    }

    fn parse_if(&mut self) -> Result<ASTNode<'a>, ParserError<'a>> {
        self.consume(TokenType::LeftParen, "'(' after 'if'")?;
        let condition = self.parse_expression()?;
        self.consume(TokenType::RightParen, "')' after condition")?;

        self.consume(TokenType::LeftBrace, "'{' to start block")?;

        let then_branch = self.parse_block();

        let else_branch = if self.match_token(TokenType::Else) {
            if self.match_token(TokenType::If) {
                let nested_if = self.parse_if()?;
                Some(vec![nested_if])
            } else {
                Some(self.parse_block())
            }
        } else {
            None
        };

        Ok(ASTNode::IfStatement {
            condition: Box::new(condition),
            then_branch,
            else_branch
        })
    }

    fn parse_while(&mut self) -> Result<ASTNode<'a>, ParserError<'a>> {
        self.consume(TokenType::LeftParen, "'(' after 'while'")?;
        let condition = self.parse_expression()?;
        self.consume(TokenType::RightParen, "')' after condition")?;

        let body = self.parse_block();

        Ok(ASTNode::WhileLoop {
            condition: Box::new(condition),
            body
        })
    }

    fn parse_for(&mut self) -> Result<ASTNode<'a>, ParserError<'a>> {
        self.consume(TokenType::LeftParen, "'(' after for")?;
        let variable = self.consume(TokenType::Identifier("".to_string()), "loop variable name")?.clone();
        self.consume(TokenType::In, "'in' after loop variable")?;

        let start = self.parse_expression()?;

        let is_inclusive = if self.match_token(TokenType::DoubleDotEqual) {
            true
        } else if self.match_token(TokenType::DoubleDot) {
            false
        } else {
            return Err(ParserError::EXPECTED_FOUND(Box::new(ExpectedFoundError {
                code: "E002",
                message: format!("expected .. or ..=, but found `{}`", self.tokens[self.current].lexeme),
                token: self.tokens[self.current].clone(),
            })));
        };
        let end = self.parse_expression()?;

        self.consume(TokenType::RightParen, "')' after range")?;

        self.consume(TokenType::LeftBrace, "'{' to start loop body")?;

        let body = self.parse_block();

        Ok(ASTNode::ForLoop {
            variable,
            start: Box::new(start),
            end: Box::new(end),
            is_inclusive,
            body
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

        let body = self.parse_block();

        Ok(ASTNode::FunctionDeclaration {
            name: name.clone(),
            parameters,
            return_type: return_type.clone(),
            body,
        })
    }

    // ========================================================================
    // 5. TYPE PARSING
    // ========================================================================

    fn parse_type(&mut self) -> Result<Box<ASTNode<'a>>, ParserError<'a>> {
        if self.match_token(TokenType::LeftBracket) {
            let start_token = self.previous().clone();

            let element_type = self.parse_type()?;

            self.consume(TokenType::Comma,"',' to separate type and array size")?;

            let size_expr = self.parse_primary()?;
            self.consume(TokenType::RightBracket, "']' to close the array")?;

            Ok(Box::new(ASTNode::ArrayType {
                element_type,
                size: Box::new(size_expr),
                token: start_token,
            }))
        } else {
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

                _ => Err(ParserError::GENERIC(Box::new(GenericError {
                    code: "E000",
                    message: "expected a type name or array type".to_string(),
                    token: current_token.clone(),
                    help: Some("consider adding a type annotation".to_string())
                })))
            }
        }
    }

    // ========================================================================
    // 6. EXPRESSION PARSING (Precedence: Lowest -> Highest)
    // ========================================================================

    fn parse_expression(&mut self) -> Result<ASTNode<'a>, ParserError<'a>> {
        self.parse_assignment()
    }

    fn parse_assignment(&mut self) -> Result<ASTNode<'a>, ParserError<'a>> {
        let target = self.parse_logical_or()?; 

        if self.match_token(TokenType::Equal) ||
            self.match_token(TokenType::PlusEqual) ||
            self.match_token(TokenType::MinusEqual) ||
            self.match_token(TokenType::StarEqual) ||
            self.match_token(TokenType::ForwardSlashEqual) ||
            self.match_token(TokenType::ModuloEqual)
        {
            let operator = self.previous().clone();
            let value = self.parse_assignment()?;
            
            return Ok(ASTNode::AssignmentExpression {
                target: Box::new(target),
                operator,
                value: Box::new(value)
            });
        }

        Ok(target)
    }

    fn parse_logical_or(&mut self) -> Result<ASTNode<'a>, ParserError<'a>> {
        let mut node = self.parse_logical_and()?;

        while self.match_token(TokenType::DoublePipe) {
            let operator = self.previous().clone();
            let right = self.parse_logical_and()?;

            node = ASTNode::BinaryExpression {
                left: Box::new(node),
                operator,
                right: Box::new(right),
            };
        }

        Ok(node)
    }

    fn parse_logical_and(&mut self) -> Result<ASTNode<'a>, ParserError<'a>> {
        let mut node = self.parse_equality()?;

        while self.match_token(TokenType::DoubleAmpersand) {
            let operator = self.previous().clone();
            let right = self.parse_equality()?;

            node = ASTNode::BinaryExpression {
                left: Box::new(node),
                operator,
                right: Box::new(right),
            };
        }

        Ok(node)
    }

    fn parse_equality(&mut self) -> Result<ASTNode<'a>, ParserError<'a>> {
        let mut node = self.parse_comparison()?;

        while self.match_token(TokenType::DoubleEqual) || self.match_token(TokenType::ExclamEqual) {
            let operator = self.previous().clone();
            let right = self.parse_comparison()?;

            node = ASTNode::BinaryExpression {
                left: Box::new(node),
                operator,
                right: Box::new(right),
            };
        }

        Ok(node)
    }

    fn parse_comparison(&mut self) -> Result<ASTNode<'a>, ParserError<'a>> {
        let mut node = self.parse_additive()?;

        while self.match_token(TokenType::LeftAngle) || self.match_token(TokenType::LessEqual) ||
            self.match_token(TokenType::RightAngle) || self.match_token(TokenType::GreaterEqual)
        {
            let operator = self.previous().clone();
            let right = self.parse_additive()?;

            node = ASTNode::BinaryExpression { 
                left: Box::new(node),
                operator,
                right: Box::new(right)
            };
        }

        Ok(node)
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
        let mut node = self.parse_unary()?;

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
                let right = self.parse_unary()?;

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

    fn parse_unary(&mut self) -> Result<ASTNode<'a>, ParserError<'a>> {
        if self.match_token(TokenType::ExclamationMark) || self.match_token(TokenType::Minus) {
            let operator = self.previous().clone();
            let right = self.parse_unary()?;

            return Ok(ASTNode::UnaryExpression {
                operator,
                right: Box::new(right),
            });
        }

        self.parse_call()
    }

    fn parse_call(&mut self) -> Result<ASTNode<'a>, ParserError<'a>> {
        let mut expr = self.parse_primary()?;

        loop {
            if self.match_token(TokenType::LeftParen) {
                let token_name = match &expr {
                    ASTNode::VariableExpression { name } => name.clone(),
                    _ => {
                        return Err(ParserError::GENERIC(Box::new(GenericError { 
                            code: "E000",
                            message: "expected function name before '('".to_string(), 
                            token: self.previous().clone(),
                            help: None
                        })))
                    }
                };
            
                expr = self.finish_parse_fn_call(token_name)?;
            } else if self.match_token(TokenType::PlusPlus) || self.match_token(TokenType::MinusMinus) {
                let operator = self.previous().clone();
                
                expr = ASTNode::PostfixUnaryExpression {
                    operator,
                    left: Box::new(expr),
                };
            } else if self.match_token(TokenType::LeftBracket) {
                let token = self.previous().clone();
                let index = self.parse_expression()?;
                
                self.consume(TokenType::RightBracket, "']' after array index")?;

                expr = ASTNode::ArrayAccess {
                    array: Box::new(expr),
                    index: Box::new(index),
                    token,
                };
            } else {
                break;
            }
        }

        Ok(expr)
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

    fn parse_primary(&mut self) -> Result<ASTNode<'a>, ParserError<'a>> {
        let current_token = &self.tokens[self.current];

        use TokenType::*;
        match &current_token.token_type {
            IntLiteral(_) | FloatLiteral(_) | StringLiteral(_) |
            CharLiteral(_)| BoolLiteral(_) => 
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

            Star | ForwardSlash | Plus | Modulo => {
                Err(ParserError::GENERIC(Box::new(GenericError {
                    code: "E004",
                    message: format!("unexpected operator `{}` found here", current_token.lexeme),
                    token: current_token.clone(),
                    help: Some("did you type an operator twice? (e.g. `**` instead of `*`)".to_string())
                })))
            }

            _ => Err(ParserError::GENERIC(Box::new(GenericError {
                code: "E000",
                message: "expected a value (number, value or condition)".to_string(),
                token: current_token.clone(),
                help: None
            }))),
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
        self.consume(TokenType::RightBrace, "'}' to close array initializer")?;

        Ok(ASTNode::ArrayInitializer {
            elements,
            token: start_token
        })
    }

    // ========================================================================
    // 7. HELPERS
    // ========================================================================

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
            Err(ParserError::EXPECTED_FOUND(Box::new(ExpectedFoundError {
                code: "E002",
                message: format!("expected {}, but found `{}`", expected, self.tokens[self.current].lexeme),
                token: self.tokens[self.current].clone(),
            })))
        }
    }

    fn peek(&self) -> &Token<'a> {
        &self.tokens[self.current]
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
