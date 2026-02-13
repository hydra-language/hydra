use lexer::{Token, TokenType};
use crate::{ASTNode, ParserError, StructSection, loader::ExternalLoader};
use errors::{generic::GenericError, expected_found::ExpectedFoundError};

pub struct Parser<'a, 'b> {
    tokens: Vec<Token<'a>>,
    current: usize,
    errors: Vec<ParserError<'a>>,
    loader: &'b mut ExternalLoader<'a>,
    allow_struct: bool,
}

impl<'a, 'b> Parser<'a, 'b> {

    // ========================================================================
    // 1. LIFECYCLE & ENTRY POINT
    // ========================================================================

    pub fn new(tokens: Vec<Token<'a>>, loader: &'b mut ExternalLoader<'a>) -> Self {
        Self {
            tokens, 
            current: 0,
            errors: Vec::new(),
            loader,
            allow_struct: true,
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
        if self.match_token(TokenType::Include) {
            return self.parse_include();
        }

        if self.match_token(TokenType::Let) || self.match_token(TokenType::Const) {
            self.parse_variable()
        } else if self.match_token(TokenType::Function) {
            self.parse_function()
        } else if self.match_token(TokenType::Struct) {
            self.parse_struct()
        } else if self.match_token(TokenType::Return) {
            self.parse_return()
        } else if self.match_token(TokenType::If) {
            self.parse_if()
        } else if self.match_token(TokenType::For) {
            self.parse_for()
        } else if self.match_token(TokenType::ForEach) {
            self.parse_foreach()
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
            let has_paren = self.match_token(TokenType::LeftParen);

            condition = Some(Box::new(self.parse_expression()?));
            
            if has_paren {
                self.consume(TokenType::RightParen, "')' after condition")?;
            }
        }

        self.consume(TokenType::Semicolon, "';' after break")?;

        Ok(ASTNode::Break { condition })
    }

    fn parse_continue(&mut self) -> Result<ASTNode<'a>, ParserError<'a>> {
        let mut condition = None;

        if self.match_token(TokenType::If) {
            let has_paren = self.match_token(TokenType::LeftParen);

            condition = Some(Box::new(self.parse_expression()?));
            
            if has_paren {
                self.consume(TokenType::RightParen, "')' after condition")?;
            }
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
        let has_paren = self.match_token(TokenType::LeftParen);

        let condition = self.parse_expression()?;

        if has_paren {
            self.consume(TokenType::RightParen, "')' after if condition")?;
        }

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
        self.allow_struct = false;
        let condition = self.parse_expression()?;
        self.allow_struct = true;

        let body = self.parse_block();

        Ok(ASTNode::WhileLoop {
            condition: Box::new(condition),
            body
        })
    }

    fn parse_for(&mut self) -> Result<ASTNode<'a>, ParserError<'a>> {
        let has_paren = self.match_token(TokenType::LeftParen);

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

        if has_paren {
            self.consume(TokenType::RightParen, "')' after range")?;
        }

        let body = self.parse_block();

        Ok(ASTNode::ForLoop {
            variable,
            start: Box::new(start),
            end: Box::new(end),
            is_inclusive,
            body
        })
    }
    
    fn parse_foreach(&mut self) -> Result<ASTNode<'a>, ParserError<'a>> {
        let has_paren = self.match_token(TokenType::LeftParen);

        let item_name = self.consume(TokenType::Identifier("".to_string()), "item name")?.clone();
        self.consume(TokenType::In, "'in' after item name")?;

        let iterable = self.parse_expression()?;

        if has_paren {
            self.consume(TokenType::RightParen, "')' after iterable")?;
        }

        let body = self.parse_block();

        Ok(ASTNode::ForEach {
            item: item_name,
            iterable: Box::new(iterable),
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

    fn parse_function_rest(&mut self, struct_context: Option<&'a str>) 
        -> Result<ASTNode<'a>, ParserError<'a>> 
    {
        let name = self.consume_identifier("expected function name")?;
        self.consume(TokenType::LeftParen, "expected '(' after function name")?;

        let mut params = Vec::new();
        if !self.check(TokenType::RightParen) {
            loop {
                // Determine if we are looking at '&self' or '&const self'
                let is_shorthand = self.check(TokenType::Ampersand) && (
                    self.check_lexeme_at(1, "self") || 
                    (self.check_at(1, TokenType::Const) && self.check_lexeme_at(2, "self"))
                );

                if is_shorthand {
                    self.advance(); // consume '&'
                    let is_const = self.match_token(TokenType::Const);
                    let self_token = self.consume_identifier("expected 'self'")?;

                    let s_name = struct_context.ok_or_else(|| {
                        ParserError::GENERIC(Box::new(GenericError {
                            code: "E002",
                            message: "cannot use 'self' shorthand outside of a struct".into(),
                            token: self_token.clone(),
                            help: None,
                        }))
                    })?;

                    let type_id = Box::new(ASTNode::TypeIdentifier {
                        type_token: Token { lexeme: s_name, ..self_token.clone() }
                    });

                    let self_type = if is_const {
                        Box::new(ASTNode::ConstReference { inner: type_id })
                    } else {
                        Box::new(ASTNode::Reference { inner: type_id })
                    };

                    params.push((self_token, self_type));
                } else {
                    let param_name = self.consume_identifier("expected parameter name")?;
                    self.consume(TokenType::Colon, "expected ':' after parameter name")?;
                    let param_type = self.parse_type()?;

                    params.push((param_name, param_type));
                }

                if !self.match_token(TokenType::Comma) {
                    break;
                }
            }
        }
        
        self.consume(TokenType::RightParen, "expected ')' after parameters")?;
        self.consume(TokenType::Arrow, "expected '->' before return type")?;
        let return_type = self.parse_type()?;
        let body = self.parse_block();

        Ok(ASTNode::FunctionDeclaration { 
            name, 
            parameters: params, 
            return_type, 
            body 
        })
    }

    fn parse_struct(&mut self) -> Result<ASTNode<'a>, ParserError<'a>> {
        let name = self.consume_identifier("expected struct name")?;
        self.consume(TokenType::LeftBrace, "expected '{' before struct body")?;

        let mut constants = Vec::new();
        let mut fields = Vec::new();
        let mut methods = Vec::new();

        let mut current_section = StructSection::NONE;

        while !self.check(TokenType::RightBrace) && !self.is_at_end() {
            if self.match_token(TokenType::Const) {
                if current_section > StructSection::CONSTANTS {
                    return Err(ParserError::GENERIC(Box::new(GenericError {
                        code: "E005",
                        message: "constants must appear before fields and methods".to_string(),
                        token: self.peek().clone(),
                        help: None
                    })));
                }

                current_section = StructSection::CONSTANTS;

                let const_name = self.consume_identifier("expected constant name")?;
                self.consume(TokenType::Colon, "expected ':' after constant name")?;
                
                let const_type = self.parse_type()?;
                self.consume(TokenType::Equal, "expected '=' in constant declaration")?;

                let value = self.parse_expression()?;
                self.consume(TokenType::Semicolon, "expected ';' after constant")?;

                constants.push(ASTNode::VariableDeclaration {
                    is_const: true,
                    name: const_name,
                    type_annotation: Some(const_type),
                    initializer: Box::new(value),
                });
            } else if self.match_token(TokenType::Function) {
                current_section = StructSection::METHODS;

                let struct_name = name.lexeme;
                methods.push(self.parse_function_rest(Some(struct_name))?);
            } else if current_section < StructSection::METHODS {
                current_section = StructSection::FIELDS;

                let field_name = self.consume_identifier("expected field name")?.clone();
                self.consume(TokenType::Colon, "expected ':'")?;

                let field_type = self.parse_type()?;
                self.consume(TokenType::Semicolon, "expected ';'")?;

                fields.push((field_name, field_type));
            } else {
                return Err(ParserError::GENERIC(Box::new(GenericError {
                    code: "E005",
                    message: "only functions are allowed in the methods section".to_string(),
                    token: self.peek().clone(),
                    help: None
                })));
            }
        }

        self.consume(TokenType::RightBrace, "expected '}' after struct body")?;

        Ok(ASTNode::StructDeclaration {
            name,
            constants,
            fields,
            methods,
        })
    }

    fn parse_struct_initializer(&mut self, name: Token<'a>) -> Result<ASTNode<'a>, ParserError<'a>> {
        self.consume(TokenType::LeftBrace, "expected '{' for struct initializer")?;
        let mut fields = Vec::new();

        if !self.check(TokenType::RightBrace) {
            loop {
                // Expect the leading '.'
                self.consume(TokenType::Dot, "expected '.' before field name")?;

                let field_name = self.consume_identifier("field name")?.clone();
                self.consume(TokenType::Equal, "expected '=' after field name")?;

                let value = self.parse_expression()?;
                fields.push((field_name, Box::new(value)));

                if !self.match_token(TokenType::Comma) {
                    break;
                }

                if self.check(TokenType::RightBrace) {
                    break;
                }
            }
        }

        self.consume(TokenType::RightBrace, "expected '}' to close struct initializer")?;

        Ok(ASTNode::StructInitializer { 
            name, 
            fields 
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

            return Ok(Box::new(ASTNode::ArrayType {
                element_type,
                size: Box::new(size_expr),
                token: start_token,
            }));
        }

        if self.match_token(TokenType::Ampersand) {
            let is_const = self.match_token(TokenType::Const);
            let inner = self.parse_type()?; // Recursive call
            
            return if is_const {
                Ok(Box::new(ASTNode::ConstReference { inner }))
            } else {
                Ok(Box::new(ASTNode::Reference { inner }))
            };
        }

        let current_token = self.peek();
        use TokenType::*;
        match &current_token.token_type {
            Const => {
                self.advance();
                self.parse_type()
            }

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
                let right = self.parse_multiplicative()?;

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
        if self.match_token(TokenType::ExclamationMark) ||
            self.match_token(TokenType::Minus) || 
            self.match_token(TokenType::Ampersand) 
        {
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
            } else if self.match_token(TokenType::DoubleColon) {
                let method_name = self.consume_identifier("method name after '::'")?;

                if self.check(TokenType::LeftParen) {
                    self.advance();

                    let args = self.finish_parse_fn_call_args()?;
                    expr = ASTNode::MethodCallExpression {
                        object: Box::new(expr),
                        method: method_name,
                        arguments: args,
                    };
                } else {
                    expr = ASTNode::MemberExpression {
                        object: Box::new(expr), 
                        property: method_name,
                    };
                }
            } else if self.match_token(TokenType::Dot) {
                let name = self.consume_identifier("property name after '.'")?;
                
                expr = ASTNode::MemberExpression {
                    object: Box::new(expr),
                    property: name,
                };
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
            } else if self.match_token(TokenType::Dot) {
                let name = if let TokenType::Identifier(_) = self.peek().token_type {
                    self.advance().clone()
                } else {
                    return Err(ParserError::GENERIC(Box::new(GenericError {
                        code: "P001",
                        message: "expected property name after '.'".to_string(),
                        token: self.peek().clone(),
                        help: None
                    })));
                };

                expr = ASTNode::MemberExpression {
                    object: Box::new(expr),
                    property: name,
                };
            } else {
                break;
            }
        }

        Ok(expr)
    }

    fn finish_parse_fn_call(&mut self, name: Token<'a>) -> Result<ASTNode<'a>, ParserError<'a>> {
        let arguments = self.finish_parse_fn_call_args()?;

        Ok(ASTNode::FunctionCallExpression {
            name, 
            arguments 
        })
    }

    fn finish_parse_fn_call_args(&mut self) -> Result<Vec<ASTNode<'a>>, ParserError<'a>> {
        let mut args = Vec::new();

        if !self.check(TokenType::RightParen) {
            loop {
                args.push(self.parse_expression()?);
                if !self.match_token(TokenType::Comma) {
                    break;
                }
            }
        }

        self.consume(TokenType::RightParen, "expected ')' after arguments")?;

        Ok(args)
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

            AnySize => {
                self.advance();
                Ok(ASTNode::Expression {
                    token: self.previous().clone(),
                })
            }

            Identifier(_) => {
                let mut name = self.advance().clone().lexeme.to_string();

                while self.match_token(TokenType::DoubleColon) {
                    let member = self.consume_identifier("expected name after '::'")?;
                    name = format!("{}::{}", name, member.lexeme);
                }

                let leaked: &'a str = Box::leak(name.into_boxed_str());

                let mut final_token = self.previous().clone();
                final_token.lexeme = leaked;

                if self.check(TokenType::LeftBrace) {
                    return self.parse_struct_initializer(final_token);
                }

                let mut final_token = self.previous().clone();
                final_token.lexeme = leaked;

                if self.allow_struct && self.check(TokenType::LeftBrace) {
                    return self.parse_struct_initializer(final_token);
                }

                Ok(ASTNode::VariableExpression { name: final_token })
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

    fn parse_include(&mut self) -> Result<ASTNode<'a>, ParserError<'a>> {
        let module_name = self.consume_identifier("expected module name")?.lexeme;
        self.consume(TokenType::DoubleColon, "expected '::' after module name")?;

        let item_name = self.consume_identifier("expected item name")?.lexeme;
        self.consume(TokenType::Semicolon, "expected ';' after include")?;

        let error_token = self.previous().clone();

        let external_declarations = self.loader.load(module_name).map_err(|e| {
            ParserError::GENERIC(Box::new(GenericError {
                code: "E006",
                message: e,
                token: error_token,
                help: None,
            }))
        })?;

        for node in external_declarations {
            match node {
                ASTNode::StructDeclaration { name, .. } if name.lexeme == item_name => return Ok(node.clone()),
                ASTNode::FunctionDeclaration { name, .. } if name.lexeme == item_name => return Ok(node.clone()),
                _ => continue,
            }
        }

        Err(ParserError::GENERIC(Box::new(GenericError {
            code: "E006",
            message: format!("item '{}' not found in '{}'", item_name, module_name),
            token: self.previous().clone(),
            help: None,
        })))
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

    fn consume_identifier(&mut self, expected: &str) -> Result<Token<'a>, ParserError<'a>> {
        if let TokenType::Identifier(_) = self.peek().token_type {
            Ok(self.advance().clone())
        } else {
            Err(ParserError::EXPECTED_FOUND(Box::new(ExpectedFoundError {
                code: "E002",
                message: format!("expected {}, found '{}'", expected, self.tokens[self.current].lexeme),
                token: self.tokens[self.current].clone(),
            })))
        }
    }

    fn parse_standard_param(&mut self) -> Result<(Token<'a>, Box<ASTNode<'a>>), ParserError<'a>> {
        let param_name = self.consume_identifier("expected parameter name")?;
        self.consume(TokenType::Colon, "expected ':' after parameter name")?;
        let param_type = self.parse_type()?;
        Ok((param_name, param_type))
    }

    fn check_at(&self, offset: usize, token_type: TokenType) -> bool {
        let pos = self.current + offset;
        if pos >= self.tokens.len() || self.tokens[pos].token_type == TokenType::EOF {
            return false;
        }

        std::mem::discriminant(&self.tokens[pos].token_type) == std::mem::discriminant(&token_type)
    }

    /// Checks the string lexeme at a specific offset from current
    fn check_lexeme_at(&self, offset: usize, lexeme: &str) -> bool {
        let pos = self.current + offset;
        if pos >= self.tokens.len() || self.tokens[pos].token_type == TokenType::EOF {
            return false;
        }
        self.tokens[pos].lexeme == lexeme
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
