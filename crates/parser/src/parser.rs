use lexer::{Token, TokenType};
use crate::{ASTNode, Annotation};
use errors::error::HydraError;

#[derive(PartialEq, PartialOrd, Clone, Copy)]
pub enum StructSection {
    NONE = 0,
    FIELDS = 1,
    CONSTANTS = 2,
    METHODS = 3,
}

pub struct Parser<'a> {
    tokens: Vec<Token<'a>>,
    current: usize,
    errors: Vec<HydraError>,
    allow_struct: bool,
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
            allow_struct: true,
        }
    }

    fn error(&self, token: &Token<'a>, code: &'static str, message: impl Into<String>) -> HydraError {
        HydraError::new(code, message, token.span)
    }

    pub fn parse(&mut self) -> Result<Vec<ASTNode<'a>>, Vec<HydraError>> {
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

    fn parse_declaration(&mut self) -> Result<Vec<ASTNode<'a>>, HydraError> {
        if self.match_token(TokenType::INCLUDE) {
            return self.parse_include();
        }

        if self.match_token(TokenType::MODULE) {
            return Ok(vec![self.parse_module()?]);
        }

        let annotations = self.parse_annotations()?;

        let is_pub = self.match_token(TokenType::PUB);

        let stmt = if self.match_token(TokenType::LET) || self.match_token(TokenType::CONST) {
            if is_pub {
                return Err(self.error(self.previous(), "P010", "pub not supported on variables"));
            }

            self.parse_variable()
        } else if self.match_token(TokenType::FN) {
            self.parse_function(annotations, is_pub)
        } else if self.match_token(TokenType::STRUCT) {
            self.parse_struct(is_pub)
        } else if self.match_token(TokenType::EXTENSION) {
            if is_pub {
                return Err(self.error(self.previous(), "P010", "pub cannot be attached to an extension block"));
            }

            self.parse_extension(is_pub)
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
            self.parse_extern(is_pub)
        } else {
            self.parse_statement()
        }?;

        Ok(vec![stmt])
    }

    // ========================================================================
    // 4. STATEMENTS & DECLARATIONS (Ordered: Basic -> Complex)
    // ========================================================================

    fn parse_variable(&mut self) -> Result<ASTNode<'a>, HydraError> {
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
            is_pub: false,
            is_const,
            name,
            type_annotation,
            initializer: Box::new(initializer),
        })
    }

    fn parse_return(&mut self) -> Result<ASTNode<'a>, HydraError> {
        let value = self.parse_expression()?;

        self.consume(TokenType::Semicolon, "';' after return value")?;

        Ok(ASTNode::ReturnStatement {
            value: Box::new(value),
        })
    }

    fn parse_break(&mut self) -> Result<ASTNode<'a>, HydraError> {
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

    fn parse_continue(&mut self) -> Result<ASTNode<'a>, HydraError> {
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

    fn parse_statement(&mut self) -> Result<ASTNode<'a>, HydraError> {
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

    fn parse_if(&mut self) -> Result<ASTNode<'a>, HydraError> {
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

    fn parse_while(&mut self) -> Result<ASTNode<'a>, HydraError> {
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

    fn parse_for(&mut self) -> Result<ASTNode<'a>, HydraError> {
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
            return Err(self.error(self.peek(), "P002", format!("expected .. or ..=, but found `{}`", self.peek().lexeme)));
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
    
    fn parse_foreach(&mut self) -> Result<ASTNode<'a>, HydraError> {
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

    fn parse_function(&mut self, annotations: Vec<Annotation>, is_pub: bool) -> Result<ASTNode<'a>, HydraError> {
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
            is_pub
        })
    }

    fn parse_extern(&mut self, is_pub: bool) -> Result<ASTNode<'a>, HydraError> {
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
            is_pub
        })
    }

    fn parse_function_rest(&mut self, extension_target: Option<Box<ASTNode<'a>>>, is_pub: bool, annotations: Vec<Annotation>) 
        -> Result<ASTNode<'a>, HydraError> 
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
                    let mut is_mut = false;

                    if is_shorthand {
                        self.advance(); // consume '&'
                        is_mut = self.match_token(TokenType::MUT); // check for 'mut'
                    }

                    let self_token = self.consume_identifier("expected 'self'")?;
                    if self_token.lexeme != "self" {
                        return Err(self.error(&self_token, "P002", format!("expected 'self', but found `{}`", self_token.lexeme)));
                    }

                    let target_type = extension_target.as_ref().unwrap();

                    let self_type = if is_shorthand {
                        Box::new(
                            ASTNode::Reference {
                                is_mut, 
                                inner: target_type.clone() 
                            }
                        )
                    } else {
                        target_type.clone()
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
            annotations: annotations,
            generic_params,
            parameters: params, 
            return_type, 
            body,
            is_extern: false,
            is_pub
        })
    }

    fn parse_generic_params(&mut self) -> Result<Vec<Token<'a>>, HydraError> {
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

    fn parse_struct(&mut self, is_pub: bool) -> Result<ASTNode<'a>, HydraError> {
        let name = self.consume_identifier("expected struct name")?;
        let generic_params = self.parse_generic_params()?;

        self.consume(TokenType::LeftBrace, "expected '{' before struct body")?;

        let mut constants = Vec::new();
        let mut fields = Vec::new();

        while !self.check(TokenType::RightBrace) && !self.is_at_end() {
            let is_member_pub = self.match_token(TokenType::PUB);

            if self.match_token(TokenType::CONST) {
                let constant_name = self.consume_identifier("expected constant name")?;
                self.consume(TokenType::Colon, "expected ':' after constant name")?;

                let constant_type = self.parse_type()?;
                self.consume(TokenType::Equal, "expected '=' in constant declaration")?;

                let value = self.parse_expression()?;
                self.consume(TokenType::Semicolon, "expected ';' after constant")?;

                constants.push(ASTNode::VariableDeclaration {
                    is_pub: is_member_pub,
                    is_const: true,
                    name: constant_name,
                    type_annotation: Some(constant_type),
                    initializer: Box::new(value),
                });
            } else if self.check(TokenType::FN) {
                return Err(self.error(self.peek(), "P011", "functions are not allowed in struct bodies")
                    .with_help("use an 'extension' block to implement functions for this struct"));
            } else {
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
            is_pub
        })
    }

    fn parse_struct_initializer(&mut self, name: ASTNode<'a>) -> Result<ASTNode<'a>, HydraError> {
        self.consume(TokenType::LeftBrace, "expected '{' for struct initializer")?;
        let mut fields = Vec::new();

        if !self.check(TokenType::RightBrace) {
            loop {
                self.consume(TokenType::Dot, "expected '.' before field name")?;
                let field_name = self.consume_identifier("field name")?.clone();
                self.consume(TokenType::Equal, "expected '=' after field name")?;
                let value = self.parse_expression()?;
                fields.push((field_name, Box::new(value)));

                if !self.match_token(TokenType::Comma) { break; }
                if self.check(TokenType::RightBrace) { break; }
            }
        }

        self.consume(TokenType::RightBrace, "expected '}' to close struct initializer")?;

        Ok(ASTNode::StructInitializer { 
            name: Box::new(name), 
            fields 
        })
    }

    fn parse_extension(&mut self, is_pub: bool) -> Result<ASTNode<'a>, HydraError> {
        let generic_params = self.parse_generic_params()?;

        let mut target = self.parse_type()?;
        let mut trait_target = None;

        if self.match_token(TokenType::ON) {
            trait_target = Some(target);
            target = self.parse_type()?;
        }

        self.consume(TokenType::LeftBrace, "expected '{' before extension body")?;

        let mut constants = Vec::new();
        let mut methods = Vec::new();

        while !self.check(TokenType::RightBrace) && !self.is_at_end() {
            let annotations = self.parse_annotations()?;

            let is_member_pub = self.match_token(TokenType::PUB);

            if self.match_token(TokenType::CONST) {
                let name = self.consume_identifier("expected constant name")?;
                self.consume(TokenType::Colon, "expected ':' after constant name")?;

                let const_type = self.parse_type()?;
                self.consume(TokenType::Equal, "expected '=' in constant declaration")?;

                let value = self.parse_expression()?;
                self.consume(TokenType::Semicolon, "expected ';' after constant")?;

                constants.push(ASTNode::VariableDeclaration {
                    is_pub: is_member_pub,
                    is_const: true,
                    name,
                    type_annotation: Some(const_type),
                    initializer: Box::new(value),
                });
            } else if self.match_token(TokenType::FN) {
                methods.push(self.parse_function_rest(Some(target.clone()), is_member_pub, annotations)?);
            } else {
                return Err(self.error(self.peek(), "P009", "only constants and functions are allowed inside extension blocks"));
            }
        }

        self.consume(TokenType::RightBrace, "expected '}' after extension body")?;

        Ok(ASTNode::ExtensionDeclaration {
            trait_target,
            target, 
            generic_params,
            constants, 
            methods 
        })
    }

    // ========================================================================
    // 5. TYPE PARSING
    // ========================================================================

    fn parse_type(&mut self) -> Result<Box<ASTNode<'a>>, HydraError> {
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
            let is_mut = self.match_token(TokenType::MUT);
            let inner = self.parse_type()?;

            return Ok(Box::new(
                ASTNode::Reference {
                    is_mut, 
                    inner 
                }
            ));
        }

        if self.match_token(TokenType::Star) {
            let is_mut = if self.match_token(TokenType::MUT) {
                true
            } else if self.match_token(TokenType::CONST) {
                false
            } else {
                return Err(self.error(self.previous(), "P012", "raw pointers must explicitly be '*mut T' or '*const T'"));
            };

            let inner = self.parse_type()?;
            
            return Ok(Box::new(ASTNode::RawPointer { is_mut, inner }));
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
                let first_token = self.advance().clone();
                let mut type_node;

                // --- NEW SEGMENTS LOGIC GOES HERE ---
                // (Note: adjust `DoubleColon` to whatever your `::` token is named)
                if self.match_token(TokenType::DoubleColon) {
                    let mut segments = vec![first_token];

                    loop {
                        let next_token = self.advance().clone();
                        if let TokenType::IDENTIFIER(_) = next_token.token_type {
                            segments.push(next_token);
                        } else {
                            return Err(self.error(&next_token, "P000", "expected identifier after '::'"));
                        }

                        if !self.match_token(TokenType::DoubleColon) {
                            break;
                        }
                    }

                    type_node = ASTNode::PathExpression { segments };
                } else {
                    // Fallback to standard single identifier
                    type_node = ASTNode::TypeIdentifier { type_token: first_token };
                }
                // ------------------------------------

                // Generic parsing remains exactly the same! 
                // Because we set `type_node` above, `base` will now capture the PathExpression 
                // if it exists, allowing things like `std::vec::Vec<i32>`
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
            }

            _ => Err(self.error(current_token, "P000", "expected a type name or array type")
                .with_help("consider adding a type annotation"))
        }
    }

    // ========================================================================
    // 6. EXPRESSION PARSING (Precedence: Lowest -> Highest)
    // ========================================================================

    fn parse_expression(&mut self) -> Result<ASTNode<'a>, HydraError> {
        self.parse_assignment()
    }

    fn parse_assignment(&mut self) -> Result<ASTNode<'a>, HydraError> {
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

    fn parse_logical_or(&mut self) -> Result<ASTNode<'a>, HydraError> {
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

    fn parse_logical_and(&mut self) -> Result<ASTNode<'a>, HydraError> {
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

    fn parse_equality(&mut self) -> Result<ASTNode<'a>, HydraError> {
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

    fn parse_comparison(&mut self) -> Result<ASTNode<'a>, HydraError> {
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

    fn parse_additive(&mut self) -> Result<ASTNode<'a>, HydraError> {
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

    fn parse_multiplicative(&mut self) -> Result<ASTNode<'a>, HydraError> {
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

    fn parse_unary(&mut self) -> Result<ASTNode<'a>, HydraError> {
        // borrowing
        if self.match_token(TokenType::Ampersand) {
            let is_mut = self.match_token(TokenType::MUT);
            let right = self.parse_unary()?;

            return Ok(ASTNode::BorrowExpression {
                is_mut,
                right: Box::new(right),
            });
        }

        // dereference
        if self.match_token(TokenType::Star) {
            let right = self.parse_unary()?;

            return Ok(ASTNode::DereferenceExpression {
                right: Box::new(right)
            });
        }

        // unary ops
        if self.match_token(TokenType::ExclamationMark) ||
           self.match_token(TokenType::Minus) 
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

    fn parse_call(&mut self) -> Result<ASTNode<'a>, HydraError> {
        let mut expr = self.parse_primary()?;
        loop {
            if self.match_token(TokenType::LeftParen) {
                expr = self.finish_parse_fn_call(expr, Vec::new())?;
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
                        if self.match_token(TokenType::RightAngle) { break; }
                        self.consume(TokenType::Comma, "expected ',' in generic args")?;
                    }

                    self.consume(TokenType::LeftParen, "expected '(' after generic arguments")?;
                    expr = self.finish_parse_fn_call(expr, generic_args)?;
                } else {
                    let next_name = self.consume_identifier("expected identifier after '::'")?.clone();

                    expr = match expr {
                        ASTNode::VariableExpression { name } => {
                            ASTNode::PathExpression { segments: vec![name, next_name] }
                        },
                        ASTNode::PathExpression { mut segments } => {
                            segments.push(next_name);
                            ASTNode::PathExpression { segments }
                        },

                        other => {
                            ASTNode::MemberExpression {
                                object: Box::new(other),
                                property: next_name
                            }
                        }
                    };
                }
            } else if self.allow_struct && self.check(TokenType::LeftBrace) {
                expr = self.parse_struct_initializer(expr)?;
            } else if self.match_token(TokenType::Dot) {
                let name = if let TokenType::IDENTIFIER(_) = self.peek().token_type {
                    self.advance().clone()
                } else {
                    return Err(self.error(self.peek(), "P001", "expected property name after '.'"));
                };
                expr = ASTNode::MemberExpression { object: Box::new(expr), property: name };
            } else if self.match_token(TokenType::PlusPlus) || self.match_token(TokenType::MinusMinus) {
                expr = ASTNode::PostfixUnaryExpression { operator: self.previous().clone(), left: Box::new(expr) };
            } else if self.match_token(TokenType::LeftBracket) {
                let token = self.previous().clone();
                let index = self.parse_expression()?;
                self.consume(TokenType::RightBracket, "']' after array index")?;
                expr = ASTNode::ArrayAccess { array: Box::new(expr), index: Box::new(index), token };
            } else {
                break;
            }
        }
        
        Ok(expr)
    }

    fn finish_parse_fn_call(&mut self, callee: ASTNode<'a>, generic_args: Vec<ASTNode<'a>>) -> Result<ASTNode<'a>, HydraError> {
        let arguments = self.finish_parse_fn_call_args()?;
        Ok(ASTNode::FunctionCallExpression { callee: Box::new(callee), arguments, generic_args })
    }

    fn finish_parse_fn_call_args(&mut self) -> Result<Vec<ASTNode<'a>>, HydraError> {
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

    fn parse_primary(&mut self) -> Result<ASTNode<'a>, HydraError> {
        let current_token = &self.tokens[self.current];

        use TokenType::*;
        match &current_token.token_type {
            IntLiteral(_) | FloatLiteral(_) | StringLiteral(_) | CharLiteral(_)| BoolLiteral(_) | ANYSIZE => {
                self.advance();
                Ok(ASTNode::Expression { token: self.previous().clone() })
            }

            IDENTIFIER(_) => {
                let name_token = self.advance().clone();
                let path_node = ASTNode::VariableExpression { name: name_token };

                if self.allow_struct && self.check(TokenType::LeftBrace) {
                    return self.parse_struct_initializer(path_node);
                }

                Ok(path_node)
            }

            LeftParen => {
                self.advance();
                let expr = self.parse_expression()?; 
                self.consume(TokenType::RightParen, "')' after expression")?;
                Ok(expr)
            }

            LeftBrace => self.parse_array_initializer(),

            Star | ForwardSlash | Plus | Modulo => {
                Err(self.error(current_token, "P004", format!("unexpected operator `{}` found here", current_token.lexeme)))
            },

            ISIZE | I8 | I16 | I32 | I64 | USIZE | U8 | U16 | U32 | U64 | F32 | F64 | CHAR | BOOL => {
                let token = self.advance().clone();
                Ok(ASTNode::TypeIdentifier { type_token: token })
            }

            _ => Err(self.error(current_token, "P000", "expected a value (number, string, or boolean)"))
        }
    }

    fn parse_array_initializer(&mut self) -> Result<ASTNode<'a>, HydraError> {
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

    fn parse_include(&mut self) -> Result<Vec<ASTNode<'a>>, HydraError> {
        let path = self.parse_module_path()?;
        let mut alias = None;
        if self.match_token(TokenType::AS) {
            alias = Some(self.consume_identifier("expected alias name after 'as'")?.clone());
        }
        self.consume(TokenType::Semicolon, "expected ';' after include statement")?;
        Ok(vec![ASTNode::IncludeStatement {
            path: Box::new(path),
            alias
        }])
    }

    fn parse_module_path(&mut self) -> Result<ASTNode<'a>, HydraError> {
        let first = self.consume_identifier("expected identifier in include path")?.clone();
        let mut segments = vec![first];

        while self.check(TokenType::DoubleColon) {
            self.advance();
            let seg = self.consume_identifier("expected identifier after '::' in path")?.clone();
            segments.push(seg);
        }

        if segments.len() == 1 {
            Ok(ASTNode::VariableExpression { name: segments.remove(0) })
        } else {
            Ok(ASTNode::PathExpression { segments })
        }
    }

    fn parse_module(&mut self) -> Result<ASTNode<'a>, HydraError> {
        self.consume(TokenType::MODULE, "expected 'module'")?;

        let mut segments = Vec::new();
        segments.push(self.consume(TokenType::IDENTIFIER("".to_string()), "expected module name")?.clone());

        while self.match_token(TokenType::DoubleColon) {
            segments.push(self.consume(TokenType::IDENTIFIER("".to_string()), "expected identifier after '::'")?.clone());
        }

        self.consume(TokenType::Semicolon, "expected ';' after module declaration")?;

        let name_node = if segments.len() == 1 {
            Box::new(ASTNode::VariableExpression { name: segments[0].clone() })
        } else {
            Box::new(ASTNode::PathExpression { segments })
        };

        Ok(ASTNode::ModuleDeclaration { name: name_node })
    }

    fn parse_annotations(&mut self) -> Result<Vec<Annotation>, HydraError> {
        let mut annotations = Vec::new();

        while self.match_token(TokenType::Hash) {
            self.consume(TokenType::LeftBracket, "expected '[' after '#' for attributes")?;
            
            let name_token = self.consume_identifier("expected annotation name")?;
            let name = name_token.lexeme.to_string();

            let mut args = Vec::new();

            if self.match_token(TokenType::LeftParen) {
                if !self.check(TokenType::RightParen) {
                    loop {
                        let arg_token = self.advance().clone();

                        if let TokenType::StringLiteral(ref s) = arg_token.token_type {
                            args.push(s.clone())
                        } else {
                            return Err(self.error(&arg_token, "P002", "expected string literal in annotation arguments"));
                        }

                        if !self.match_token(TokenType::Comma) {
                            break;
                        }
                    }
                }

                self.consume(TokenType::RightParen, "expected ')' after annotation arguments")?;
            }
            
            self.consume(TokenType::RightBracket, "expected ']' to close attribute")?;

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

    fn consume(&mut self, token: TokenType, expected: &str) -> Result<&Token<'a>, HydraError> {
        if self.check(token) {
            Ok(self.advance())
        } else {
            Err(self.error(self.peek(), "P002", format!("expected {}, but found `{}`", expected, self.peek().lexeme)))
        }
    }

    fn consume_identifier(&mut self, expected: &str) -> Result<Token<'a>, HydraError> {
        if let TokenType::IDENTIFIER(_) = self.peek().token_type {
            Ok(self.advance().clone())
        } else {
            Err(self.error(self.peek(), "P002", format!("expected {}, found '{}'", expected, self.peek().lexeme)))
        }
    }

    fn parse_standard_param(&mut self) -> Result<(Token<'a>, Box<ASTNode<'a>>), HydraError> {
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
