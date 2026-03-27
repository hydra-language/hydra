use lexer::{Token, TokenType};
use crate::{ASTNode, Annotation, ParserError, loader::ExternalLoader};
use errors::{expected_found::ExpectedFoundError, generic::{self, GenericError}};

#[derive(PartialEq, PartialOrd, Clone, Copy)]
pub enum StructSection {
    NONE = 0,
    FIELDS = 1,
    CONSTANTS = 2,
    METHODS = 3,
}

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
                Ok(stmts) => statements.extend(stmts),
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
                TokenType::FN | TokenType::LET | TokenType::CONST | 
                TokenType::FOR | TokenType::IF | TokenType::WHILE | TokenType::RETURN => return,
                
                _ => {}
            }

            self.advance();
        }
    }

    // ========================================================================
    // 3. DECLARATION DISPATCHER
    // ========================================================================

    fn parse_declaration(&mut self) -> Result<Vec<ASTNode<'a>>, ParserError<'a>> {
        if self.match_token(TokenType::INCLUDE) {
            return self.parse_include();
        }

        let annotations = self.parse_annotations()?;

        let stmt = if self.match_token(TokenType::LET) || self.match_token(TokenType::CONST) {
            self.parse_variable()
        } else if self.match_token(TokenType::FN) {
            self.parse_function(annotations)
        } else if self.match_token(TokenType::STRUCT) {
            self.parse_struct()
        } else if self.match_token(TokenType::EXTENSION) {
            self.parse_extension()
        } else if self.match_token(TokenType::RETURN) {
            self.parse_return()
        } else if self.match_token(TokenType::IF) {
            self.parse_if()
        } else if self.match_token(TokenType::FOR) {
            self.parse_for()
        } else if self.match_token(TokenType::FOREACH) {
            self.parse_foreach()
        } else if self.match_token(TokenType::WHILE) {
            self.parse_while()
        } else if self.match_token(TokenType::BREAK) {
            self.parse_break()
        } else if self.match_token(TokenType::CONTINUE) {
            self.parse_continue()
        } else if self.match_token(TokenType::EXTERN) {
            self.parse_extern()
        } else {
            self.parse_statement()
        }?;

        Ok(vec![stmt])
    }

    // ========================================================================
    // 4. STATEMENTS & DECLARATIONS (Ordered: Basic -> Complex)
    // ========================================================================

    fn parse_variable(&mut self) -> Result<ASTNode<'a>, ParserError<'a>> {
        let is_const = self.previous().token_type == TokenType::CONST;
        let name = self.consume(TokenType::IDENTIFIER("".to_string()), "variable name")?.clone();

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

        if self.match_token(TokenType::IF) {
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

        if self.match_token(TokenType::IF) {
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

        let mut statements = Vec::new();

        while !self.check(TokenType::RightBrace) && !self.is_at_end() {
            match self.parse_declaration() {
                Ok(stmts) => statements.extend(stmts),
                Err(e) => {
                    self.errors.push(e);
                    self.synchronize();
                }
            }
        }

        if let Err(e) = self.consume(TokenType::RightBrace, "'}' to end block") {
            self.errors.push(e);
        }

        statements
    }

    fn parse_if(&mut self) -> Result<ASTNode<'a>, ParserError<'a>> {
        let has_paren = self.match_token(TokenType::LeftParen);

        if !has_paren { self.allow_struct = false; }
        let condition = self.parse_expression()?;
        if !has_paren { self.allow_struct = true; }

        if has_paren {
            self.consume(TokenType::RightParen, "')' after if condition")?;
        }

        let then_branch = self.parse_block();

        let else_branch = if self.match_token(TokenType::ELSE) {
            if self.match_token(TokenType::IF) {
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
        let has_paren = self.match_token(TokenType::LeftParen);        
        
        if !has_paren { self.allow_struct = false; }
        let condition = self.parse_expression()?;
        if !has_paren { self.allow_struct = true; }

        if has_paren {
            self.consume(TokenType::RightParen, "expected ')' after while condition")?;
        }

        let body = self.parse_block();

        Ok(ASTNode::WhileLoop {
            condition: Box::new(condition),
            body
        })
    }

    fn parse_for(&mut self) -> Result<ASTNode<'a>, ParserError<'a>> {
        let has_paren = self.match_token(TokenType::LeftParen);

        if !has_paren { self.allow_struct = false; }
        let variable = self.consume(TokenType::IDENTIFIER("".to_string()), "loop variable name")?.clone();
        self.consume(TokenType::IN, "'in' after loop variable")?;

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
        if !has_paren { self.allow_struct = true; }

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

        if !has_paren { 
            self.allow_struct = false; 
        }

        let item_name = self.consume(TokenType::IDENTIFIER("".to_string()), "item name")?.clone();
        self.consume(TokenType::IN, "'in' after item name")?;

        let iterable = self.parse_expression()?;

        if !has_paren {
            self.allow_struct = true;
        }

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

    fn parse_function(&mut self, annotations: Vec<Annotation>) -> Result<ASTNode<'a>, ParserError<'a>> {
        let name = self.consume(TokenType::IDENTIFIER("".to_string()), "function name")?.clone();

        let generic_params = self.parse_generic_params()?;

        self.consume(TokenType::LeftParen, "'(' after function name")?;

        let mut parameters = Vec::new();
        if !self.check(TokenType::RightParen) {
            loop {
                let param_name = self.consume(TokenType::IDENTIFIER("".to_string()), "parameter name")?.clone();
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

        let return_type = self.parse_type()?;

        let is_bodyless = annotations.iter()
            .any(|a| matches!(a.name.as_str(), "intrinsic" | "builtin")
        );

        let body = if is_bodyless {
            self.consume(TokenType::Semicolon, "expected ';' after intrinsic function declaration")?;
            Vec::new()
        } else {
            self.parse_block()
        };

        Ok(ASTNode::FunctionDeclaration {
            name: name.clone(),
            annotations,
            generic_params,
            parameters,
            return_type,
            body,
            is_extern: false,
        })
    }

    fn parse_extern(&mut self) -> Result<ASTNode<'a>, ParserError<'a>> {
        self.consume(TokenType::FN, "expected 'fn' after 'extern'")?;

        let name = self.consume(TokenType::IDENTIFIER("".to_string()), "function name")?.clone();
        let generic_params = self.parse_generic_params()?;

        self.consume(TokenType::LeftParen, "'(' after function name")?;

        let mut parameters = Vec::new();
        if !self.check(TokenType::RightParen) {
            loop {
                let param_name = self.consume(TokenType::IDENTIFIER("".to_string()), "parameter name")?.clone();
                self.consume(TokenType::Colon, "':' after parameter name")?;
                let param_type = self.parse_type()?;

                parameters.push((param_name, param_type));

                if !self.match_token(TokenType::Comma) {
                    break;
                }
            }
        }

        self.consume(TokenType::RightParen, "')' after parameters")?;
        self.consume(TokenType::Arrow, "-> after ')'")?;

        let return_type = self.parse_type()?;
        self.consume(TokenType::Semicolon, "expected ';' after extern declaration")?;

        Ok(ASTNode::FunctionDeclaration {
            name,
            annotations: Vec::new(),
            generic_params,
            parameters,
            return_type,
            body: Vec::new(),
            is_extern: true,
        })
    }

    fn parse_function_rest(&mut self, struct_context: Option<&'a str>) 
        -> Result<ASTNode<'a>, ParserError<'a>> 
    {
        let name = self.consume_identifier("expected function name")?;

        let generic_params = self.parse_generic_params()?;

        self.consume(TokenType::LeftParen, "expected '(' after function name")?;

        let mut params = Vec::new();
        if !self.check(TokenType::RightParen) {
            loop {
                let is_shorthand = self.check(TokenType::Ampersand);
                let is_value_ref = !is_shorthand && self.peek().lexeme == "self";

                if is_shorthand || is_value_ref {
                    let mut is_const = false;
                    
                    if is_shorthand {
                        self.advance();
                        is_const = self.match_token(TokenType::CONST);
                    }

                    let self_token = self.consume_identifier("expected 'self'")?;
                    if self_token.lexeme != "self" {
                        return Err(ParserError::EXPECTED_FOUND(Box::new(ExpectedFoundError {
                            code: "E002",
                            message: format!("expected 'self', but found `{}`", self_token.lexeme),
                            token: self_token,
                        })));
                    }

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

                    let self_type = if is_shorthand {
                        if is_const {
                            Box::new(ASTNode::ConstReference { inner: type_id })
                        } else {
                            Box::new(ASTNode::Reference { inner: type_id })
                        }
                    } else {
                            type_id
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
            annotations: Vec::new(),
            generic_params,
            parameters: params, 
            return_type, 
            body,
            is_extern: false,
        })
    }

    fn parse_generic_params(&mut self) -> Result<Vec<Token<'a>>, ParserError<'a>> {
        let mut params = Vec::new();
        if self.match_token(TokenType::LeftAngle) { // <
            loop {
                params.push(self.consume_identifier("expected generic parameter name")?);
                
                if self.match_token(TokenType::RightAngle) { // >
                    break;
                }
                self.consume(TokenType::Comma, "expected comma between generic parameters")?;
            }
        }
        Ok(params)
    }

    fn parse_struct(&mut self) -> Result<ASTNode<'a>, ParserError<'a>> {
        let name = self.consume_identifier("expected struct name")?;

        let generic_params = self.parse_generic_params()?;

        self.consume(TokenType::LeftBrace, "expected '{' before struct body")?;

        let mut constants = Vec::new();
        let mut fields = Vec::new();
        let mut methods = Vec::new();

        let mut current_section = StructSection::NONE;

        while !self.check(TokenType::RightBrace) && !self.is_at_end() {
            if self.match_token(TokenType::CONST) {
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
            } else if self.match_token(TokenType::FN) {
                current_section = StructSection::METHODS;

                let struct_name = name.lexeme;
                methods.push(self.parse_function_rest(Some(struct_name))?);
            } else {
                if current_section > StructSection::FIELDS {
                    return Err(ParserError::GENERIC(Box::new(GenericError {
                        code: "E005",
                        message: "fields must appear before constants and methods".to_string(),
                        token: self.peek().clone(),
                        help: None
                    })));
                }

                current_section = StructSection::FIELDS;

                let field_name = self.consume_identifier("expected field name")?.clone();
                self.consume(TokenType::Colon, "expected ':'")?;

                let field_type = self.parse_type()?;
                self.consume(TokenType::Semicolon, "expected ';'")?;

                fields.push((field_name, field_type));
            }
        }

        self.consume(TokenType::RightBrace, "expected '}' after struct body")?;

        Ok(ASTNode::StructDeclaration {
            name,
            generic_params,
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

    fn parse_extension(&mut self) -> Result<ASTNode<'a>, ParserError<'a>> {
        let target = self.parse_type()?;

        let type_name = match &*target {
            ASTNode::TypeIdentifier { type_token } => type_token.lexeme,
            _ => {
                return Err(ParserError::GENERIC(Box::new(GenericError {
                    code: "E003",
                    message: "extensions current only support primitives".to_string(),
                    token: self.previous().clone(),
                    help: Some("try extending a primitive like 'i32' or 'f64'".to_string())
                })));
            }
        };

        self.consume(TokenType::LeftBrace, "expected '{' before extension body")?;

        let mut constants = Vec::new();
        let mut methods = Vec::new();

        while !self.check(TokenType::RightBrace) && !self.is_at_end() {
            if self.match_token(TokenType::CONST) {
                let name = self.consume_identifier("expected constant name")?;
                self.consume(TokenType::Colon, "expected ':' after constant name")?;

                let const_type = self.parse_type()?;
                self.consume(TokenType::Equal, "expected '=' in constant declaration")?;

                let value = self.parse_expression()?;
                self.consume(TokenType::Semicolon, "expected ';' after constant")?;

                constants.push(ASTNode::VariableDeclaration {
                    is_const: true,
                    name,
                    type_annotation: Some(const_type),
                    initializer: Box::new(value),
                });
            } else if self.match_token(TokenType::FN) {
                methods.push(self.parse_function_rest(Some(type_name))?);
            } else {
                return Err(ParserError::GENERIC(Box::new(GenericError {
                    code: "E009",
                    message: "only constants and functions are allowed inside extension blocks".to_string(),
                    token: self.peek().clone(),
                    help: None
                })));
            }
        }

        self.consume(TokenType::RightBrace, "expected '}' after extension body")?;

        Ok(ASTNode::ExtensionDeclaration { 
            target, 
            constants, 
            methods 
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
            let is_const = self.match_token(TokenType::CONST);
            let inner = self.parse_type()?; // Recursive call
            
            return if is_const {
                Ok(Box::new(ASTNode::ConstReference { inner }))
            } else {
                Ok(Box::new(ASTNode::Reference { inner }))
            };
        }

        if self.match_token(TokenType::Star) {
            let inner = self.parse_type()?;
            
            return Ok(Box::new(ASTNode::Pointer { inner }));
        }

        let current_token = self.peek();
        use TokenType::*;
        match &current_token.token_type {
            CONST => {
                self.advance();
                self.parse_type()
            }

            IDENTIFIER(_) |
            ISIZE | I8 | I16 | I32 | I64 | 
            USIZE | U8 | U16 | U32 | U64 |
            F32 | F64 | CHAR | BOOL => {
                let type_token = self.advance().clone();
                let mut type_node = ASTNode::TypeIdentifier { type_token };

                if self.match_token(TokenType::LeftAngle) {
                    let mut args = Vec::new();

                    loop {
                        args.push(*self.parse_type()?);

                        if self.match_token(TokenType::RightAngle) {
                            break;
                        }

                        self.consume(TokenType::Comma, "expected ',' between generic types")?;
                    }

                    type_node = ASTNode::GenericType { 
                        base: Box::new(type_node), 
                        args 
                    };
                }

                Ok(Box::new(type_node))
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
            self.match_token(TokenType::Ampersand) || 
            self.match_token(TokenType::Star)
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
            
                expr = self.finish_parse_fn_call(token_name, Vec::new())?;
            } else if self.match_token(TokenType::AS) {
                let target = self.parse_type()?;

                expr = ASTNode::CastExpression {
                    value: Box::new(expr),
                    target: Box::new(*target),
                };
            } else if self.match_token(TokenType::DoubleColon) {
                if self.match_token(TokenType::LeftAngle) {
                    let mut generic_args = Vec::new();

                    loop {
                        generic_args.push(*self.parse_type()?);
                        
                        if self.match_token(TokenType::RightAngle) {
                            break;
                        }

                        self.consume(TokenType::Comma, "expected ',' in generic args")?;
                    }

                    if self.match_token(TokenType::DoubleColon) {
                        let method_name = self.consume_identifier("expected method name after '::'")?;
                        self.consume(TokenType::LeftParen, "expected '(' after method name")?;

                        let args = self.finish_parse_fn_call_args()?;

                        expr = ASTNode::MethodCallExpression {
                            object: Box::new(expr),
                            method: method_name,
                            arguments: args,
                            generic_args,
                        };
                    } else {
                        self.consume(TokenType::LeftParen, "expected '(' after generic args")?;

                        let args = self.finish_parse_fn_call_args()?;

                        expr = match expr {
                            ASTNode::VariableExpression { name } => {
                                ASTNode::FunctionCallExpression {
                                    name,
                                    arguments: args,
                                    generic_args,
                                }
                            },

                            ASTNode::MemberExpression { object, property } => {
                                ASTNode::MethodCallExpression {
                                    object,
                                    method: property,
                                    arguments: args,
                                    generic_args,
                                }
                            },

                            _ => return Err(ParserError::GENERIC(Box::new(GenericError {
                                code: "E007",
                                message: "generic call ::<T> must follow a name or member access".to_string(),
                                token: self.previous().clone(),
                                help: None
                            })))
                        };
                    }
                } else {
                    let method_name = self.consume_identifier("method name after '::'")?;

                    if self.check(TokenType::LeftParen) {
                        self.advance();

                        let args = self.finish_parse_fn_call_args()?;

                        expr = ASTNode::MethodCallExpression {
                            object: Box::new(expr),
                            method: method_name,
                            arguments: args,
                            generic_args: Vec::new(),
                        };
                    } else {
                        expr = ASTNode::MemberExpression {
                            object: Box::new(expr),
                            property: method_name
                        };
                    }
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
                let name = if let TokenType::IDENTIFIER(_) = self.peek().token_type {
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

    fn finish_parse_fn_call(&mut self, name: Token<'a>, generic_args: Vec<ASTNode<'a>>) 
        -> Result<ASTNode<'a>, ParserError<'a>> 
    {
        let arguments = self.finish_parse_fn_call_args()?;

        Ok(ASTNode::FunctionCallExpression {
            name, 
            arguments,
            generic_args
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

            ANYSIZE => {
                self.advance();
                Ok(ASTNode::Expression {
                    token: self.previous().clone(),
                })
            }

            IDENTIFIER(_) => {
                let mut name = self.advance().clone().lexeme.to_string();

                while self.check(TokenType::DoubleColon) && !self.check_at(1, TokenType::LeftAngle) {
                    self.advance();

                    let member = self.consume_identifier("expected name after '::'")?;
                    name = format!("{}::{}", name, member.lexeme);
                }

                let leaked: &'a str = Box::leak(name.into_boxed_str());

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

            ISIZE | I8 | I16 | I32 | I64 | USIZE | U8 | U16 | U32 | U64 | F32 | F64 | CHAR | BOOL => {
                let token = self.advance().clone();
                Ok(ASTNode::TypeIdentifier { type_token: token })
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

                if self.check(TokenType::RightBrace) {
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

    fn parse_include(&mut self) -> Result<Vec<ASTNode<'a>>, ParserError<'a>> {
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
                help: Some("fix errors the errors pal".to_string()),
            }))
        })?;

        let mut included_nodes = Vec::new();
        let mut found_item = false;

        for node in external_declarations {
            match node {
                ASTNode::StructDeclaration { name, .. } if name.lexeme == item_name => {
                    included_nodes.push(node.clone());
                    found_item = true;
                }

                ASTNode::FunctionDeclaration { name, .. } if name.lexeme == item_name => {
                    included_nodes.push(node.clone());
                    found_item = true;
                }

                ASTNode::FunctionDeclaration { is_extern: true, .. } => {
                    included_nodes.push(node.clone());
                }

                ASTNode::FunctionDeclaration { ref annotations, .. } 
                    if annotations.iter().any(|a| matches!(a.name.as_str(), "intrinsic" | "builtin")) => 
                {
                    included_nodes.push(node.clone());
                }

                _ => continue,
            }
        }


        if !found_item {
            return Err(ParserError::GENERIC(Box::new(GenericError {
                code: "E006",
                message: format!("item '{}' not found in '{}'", item_name, module_name),
                token: self.previous().clone(),
                help: None,
            })));
        }

        Ok(included_nodes)
    }

    fn parse_annotations(&mut self) -> Result<Vec<Annotation>, ParserError<'a>> {
        let mut annotations = Vec::new();

        while self.match_token(TokenType::Hash) {
            let name_token = self.consume_identifier("expected annotation name after '#'")?;
            let name = name_token.lexeme.to_string();

            let mut args = Vec::new();

            if self.match_token(TokenType::LeftParen) {
                if !self.check(TokenType::RightParen) {
                    loop {
                        let arg_token = self.advance().clone();

                        if let TokenType::StringLiteral(ref s) = arg_token.token_type {
                            args.push(s.clone())
                        } else {
                            return Err(ParserError::GENERIC(Box::new(GenericError {
                                code: "E002",
                                message: "expected string literal in annotation arguments".to_string(),
                                token: arg_token,
                                help: None,
                            })));
                        }

                        if !self.match_token(TokenType::Comma) {
                            break;
                        }
                    }
                }

                self.consume(TokenType::RightParen, "expected ')' after annotation arguments")?;
            }

            annotations.push(Annotation {
                name,
                args
            });
        }

        Ok(annotations)
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
        if let TokenType::IDENTIFIER(_) = self.peek().token_type {
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
            (IDENTIFIER(_), IDENTIFIER(_)) => true,
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
