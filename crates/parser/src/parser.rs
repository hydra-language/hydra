use lexer::{Token, TokenType};
use crate::ast::*;
use errors::error::HydraError;

pub struct Parser {
    tokens: Vec<Token>,
    current: usize,
    errors: Vec<HydraError>,
    source_id: u32,
   
    // used to generate unique ids for every syntax node 
    // so the semantic analyzer can build side-tables later.
    next_node_id: u32,
    
    // used to prevent parsing struct initializers inside if-conditions
    allow_struct: bool,

    pub headers_only: bool,
}

impl Parser {

    // ========================================================================
    // 1. LIFECYLE AND ENTRY POINT
    // ========================================================================

    pub fn new(tokens: Vec<Token>, source_id: u32) -> Self {
        Self {
            tokens, 
            current: 0,
            source_id,
            errors: Vec::new(),
            next_node_id: 1, // start at 1 so 0 can be reserved for invalid/null if needed
            allow_struct: true,
            headers_only: false,
        }
    }

    // create new node id for each ast node
    fn next_node_id(&mut self) -> NodeID {
        let id = self.next_node_id as u64;
        self.next_node_id += 1;

        let source_id = self.source_id as u64;

        NodeID((source_id << 32) | id)
    }

    fn error(&self, token: &Token, code: &'static str, message: impl Into<String>) -> HydraError {
        HydraError::new(code, message, token.span)
    }

    // the main entry point for the compiler. a file is just a list of items.
    pub fn parse(&mut self) -> Result<Vec<Item>, Vec<HydraError>> {
        let mut items = Vec::new();

        while !self.is_at_end() {
            match self.parse_item() {
                Ok(item) => items.push(item),
                Err(e) => {
                    self.errors.push(e);
                    self.synchronize_item();
                }
            }
        }

        if self.errors.is_empty() {
            Ok(items)
        } else {
            Err(self.errors.clone())
        }
    }

    // ========================================================================
    // 2. ERROR SYNCHRONIZATION
    // ========================================================================

    // skip tokens until a keyword that looks like the start of a new item is found
    fn synchronize_item(&mut self) {
        self.advance();

        while !self.is_at_end() {
            match self.peek().token_type {
                TokenType::FN | TokenType::STRUCT | TokenType::TRAIT | 
                TokenType::EXTENSION | TokenType::INCLUDE | TokenType::PUB => return,
                _ => {}
            }
            self.advance();
        }
    }

    // skips to next statement inside block in event of error
    fn synchronize_stmt(&mut self) {
        self.advance();

        while !self.is_at_end() {
            if self.previous().token_type == TokenType::Semicolon {
                return;
            }
            match self.peek().token_type {
                TokenType::LET | TokenType::CONST | TokenType::FOR | 
                TokenType::IF | TokenType::WHILE | TokenType::RETURN => return,
                _ => {}
            }
            self.advance();
        }
    }

    // ========================================================================
    // 3. TOP LEVEL DECLARATIONS
    // ========================================================================

    fn parse_item(&mut self) -> Result<Item, HydraError> {
        if self.match_token(TokenType::INCLUDE) {
            return self.parse_include();
        }

        let annotations = self.parse_annotations()?;
        let is_pub = self.match_token(TokenType::PUB);

        if self.match_token(TokenType::STRUCT) {
            self.parse_struct(is_pub, annotations)
        } else if self.match_token(TokenType::TRAIT) {
            self.parse_trait(is_pub, annotations)
        } else if self.match_token(TokenType::EXTENSION) {
            if is_pub {
                return Err(self.error(self.previous(), "P010", "pub cannot be attached to an extension block"));
            }
            self.parse_extension(annotations)
        } else if self.match_token(TokenType::FN) {
            let func = self.parse_function_decl(is_pub, annotations, false)?;
            Ok(Item::Function(func))
        } else if self.match_token(TokenType::EXTERN) {
            self.consume(TokenType::FN, "expected 'fn' after 'extern'")?;
            let func = self.parse_function_decl(is_pub, annotations, true)?;
            Ok(Item::Function(func))
        } else {
            // if we hit this, the user typed something like a floating variable or expression
            Err(self.error(self.peek(), "P013", "expected a top-level item (struct, fn, trait, or extension)"))
        }
    }

    fn parse_include(&mut self) -> Result<Item, HydraError> {
        let id = self.next_node_id();
        let first_token = self.consume_identifier("expected module path")?.clone();
        let mut segments = vec![first_token];

        while self.check(TokenType::DoubleColon) {
            if self.check_at(1, TokenType::LeftBrace) {
                break;
            }
            self.advance();
            segments.push(self.consume_identifier("expected identifier after '::'")?.clone());
        }

        let path = Type::Path { id: self.next_node_id(), segments };

        let mut symbols = None;
        if self.match_token(TokenType::DoubleColon) {
            self.consume(TokenType::LeftBrace, "expected '{' for selective include")?;
            let mut syms = Vec::new();
            if !self.check(TokenType::RightBrace) {
                loop {
                    syms.push(self.consume_identifier("expected symbol name")?.clone());
                    if !self.match_token(TokenType::Comma) { break; }
                }
            }
            self.consume(TokenType::RightBrace, "expected '}' after symbols")?;
            symbols = Some(syms);
        }

        let mut alias = None;
        if symbols.is_none() && self.match_token(TokenType::AS) {
            alias = Some(self.consume_identifier("expected alias name after 'as'")?.clone());
        }

        self.consume(TokenType::Semicolon, "expected ';' after include statement")?;

        Ok(Item::Include(IncludeDecl { id, path, symbols, alias }))
    }

    fn parse_struct(&mut self, is_pub: bool, _annotations: Vec<Annotation>) -> Result<Item, HydraError> {
        let id = self.next_node_id();
        let name = self.consume_identifier("expected struct name")?.clone();
        let generic_params = self.parse_generic_params()?;
        let where_clause = self.parse_where_clause()?;

        self.consume(TokenType::LeftBrace, "expected '{' before struct body")?;

        let mut constants = Vec::new();
        let mut fields = Vec::new();

        while !self.check(TokenType::RightBrace) && !self.is_at_end() {
            let _is_member_pub = self.match_token(TokenType::PUB); // ignoring for now, could add to field tuple later

            if self.match_token(TokenType::CONST) {
                let constant_node = self.parse_variable_decl(true)?;
                constants.push(constant_node);
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

        Ok(Item::Struct(StructDecl {
            id, name, generic_params, where_clause, constants, fields, is_pub
        }))
    }

    fn parse_trait(&mut self, is_pub: bool, _annotations: Vec<Annotation>) -> Result<Item, HydraError> {
        let id = self.next_node_id();
        let name = self.consume_identifier("expected trait name")?.clone();

        self.consume(TokenType::LeftBrace, "expected '{' before trait body")?;
        let mut methods = Vec::new();

        while !self.check(TokenType::RightBrace) && !self.is_at_end() {
            let annotations = self.parse_annotations()?;
            let is_method_pub = self.match_token(TokenType::PUB);
            
            self.consume(TokenType::FN, "expected 'fn' for trait method")?;
            
            // tell parse_function_decl this is a trait method so it expects no body
            let method = self.parse_function_decl(is_method_pub, annotations, true)?;
            methods.push(method);
        }

        self.consume(TokenType::RightBrace, "expected '}' after trait body")?;

        Ok(Item::Trait(TraitDecl { id, name, methods, is_pub }))
    }

    fn parse_extension(&mut self, _annotations: Vec<Annotation>) -> Result<Item, HydraError> {
        let id = self.next_node_id();
        let generic_params = self.parse_generic_params()?;

        let mut target_type = self.parse_type()?;
        let mut target_trait = None;

        // if 'on' first type was actually the trait
        if self.match_token(TokenType::ON) {
            target_trait = Some(target_type);
            target_type = self.parse_type()?;
        }

        let where_clause = self.parse_where_clause()?;

        self.consume(TokenType::LeftBrace, "expected '{' before extension body")?;

        let mut constants = Vec::new();
        let mut methods = Vec::new();

        while !self.check(TokenType::RightBrace) && !self.is_at_end() {
            let method_annotations = self.parse_annotations()?;
            let is_member_pub = self.match_token(TokenType::PUB);

            if self.match_token(TokenType::CONST) {
                let constant_node = self.parse_variable_decl(true)?;
                constants.push(constant_node);
            } else if self.match_token(TokenType::FN) {
                // extension methods have bodies, so pass false for 'is_extern_or_trait'
                methods.push(self.parse_function_decl(is_member_pub, method_annotations, false)?);
            } else {
                return Err(self.error(self.peek(), "P009", "only constants and functions are allowed inside extension blocks"));
            }
        }

        self.consume(TokenType::RightBrace, "expected '}' after extension body")?;

        Ok(Item::Extension(ExtensionDecl {
            id, target_trait, target_type, generic_params, where_clause, constants, methods 
        }))
    }

    // standard fns, externs and trait methods
    fn parse_function_decl(&mut self, is_pub: bool, annotations: Vec<Annotation>, is_extern_or_trait: bool) 
        -> Result<FunctionDecl, HydraError> 
    {
        let id = self.next_node_id();
        let name = self.consume_identifier("expected function name")?.clone();
        let generic_params = self.parse_generic_params()?;

        self.consume(TokenType::LeftParen, "expected '(' after function name")?;
        let mut parameters = Vec::new();
        
        if !self.check(TokenType::RightParen) {
            loop {
                // shorthand check for `&self` or `&mut self` or `self`
                let is_shorthand = self.check(TokenType::Ampersand);
                let is_value_ref = !is_shorthand && self.peek().lexeme == "self";

                if is_shorthand || is_value_ref {
                    let mut is_mut = false;
                    if is_shorthand {
                        self.advance(); // consume '&'
                        is_mut = self.match_token(TokenType::MUT);
                    }

                    let self_token = self.consume_identifier("expected 'self'")?.clone();
                    if self_token.lexeme != "self" {
                        return Err(self.error(&self_token, "P002", format!("expected 'self', found `{}`", self_token.lexeme)));
                    }

                    // for `self`, putting a generic Type::Path of "Self" as the type. 
                    // semantic analyzer will resolve "Self" properly later
                    let self_type = Type::Path {
                        id: self.next_node_id(),
                        segments: vec![Token { token_type: TokenType::IDENTIFIER("Self".to_string()), lexeme: "Self".to_string(), span: self_token.span }]
                    };

                    let final_type = if is_shorthand {
                        Type::Borrow { id: self.next_node_id(), is_mut, inner: Box::new(self_type) }
                    } else {
                        self_type
                    };

                    parameters.push((self_token, final_type));
                } else {
                    let param_name = self.consume_identifier("expected parameter name")?.clone();
                    self.consume(TokenType::Colon, "expected ':' after parameter name")?;
                    let param_type = self.parse_type()?;
                    parameters.push((param_name, param_type));
                }

                if !self.match_token(TokenType::Comma) { break; }
            }
        }

        self.consume(TokenType::RightParen, "expected ')' after parameters")?;
        
        let mut return_type = None;
        if self.match_token(TokenType::Arrow) {
            return_type = Some(self.parse_type()?);
        }

        let where_clause = self.parse_where_clause()?;

        let is_bodyless = is_extern_or_trait || annotations.iter().any(|a| matches!(a.name.as_str(), "intrinsic" | "builtin"));

        let body = if is_bodyless {
            self.consume(TokenType::Semicolon, "expected ';' after bodyless function declaration")?;
            None
        } else if self.headers_only {
            self.skip_block()?;
            None
        } else {
            Some(self.parse_block()?)
        };

        Ok(FunctionDecl {
            id, 
            name, 
            annotations, 
            generic_params, 
            parameters, 
            return_type, 
            where_clause, 
            body, 
            is_extern: is_extern_or_trait, 
            is_pub
        })
    }

    // ========================================================================
    // 4. STATEMENTS AND BLOCKS
    // ========================================================================

    fn parse_block(&mut self) -> Result<Block, HydraError> {
        let id = self.next_node_id();
        self.consume(TokenType::LeftBrace, "expected '{' to start block")?;

        let mut statements = Vec::new();

        while !self.check(TokenType::RightBrace) && !self.is_at_end() {
            match self.parse_stmt() {
                Ok(stmt) => statements.push(stmt),
                Err(e) => {
                    self.errors.push(e);
                    self.synchronize_stmt();
                }
            }
        }

        self.consume(TokenType::RightBrace, "expected '}' to end block")?;

        Ok(Block { id, statements })
    }

    fn parse_stmt(&mut self) -> Result<Stmt, HydraError> {
        if self.match_token(TokenType::LET) {
            self.parse_variable_decl(false)
        } else if self.match_token(TokenType::CONST) {
            self.parse_variable_decl(true)
        } else if self.match_token(TokenType::RETURN) {
            self.parse_return_stmt()
        } else if self.match_token(TokenType::BREAK) {
            self.parse_break_stmt()
        } else if self.match_token(TokenType::CONTINUE) {
            self.parse_continue_stmt()
        } else {
            // fallback to expression statement
            let expr = self.parse_expression()?;
            self.consume(TokenType::Semicolon, "expected ';' after expression")?;
            Ok(Stmt::Expr(Box::new(expr)))
        }
    }

    fn parse_variable_decl(&mut self, is_const: bool) -> Result<Stmt, HydraError> {
        let id = self.next_node_id();
        let name = self.consume_identifier("expected variable name")?.clone();

        let mut type_annotation = None;
        if self.match_token(TokenType::Colon) {
            type_annotation = Some(self.parse_type()?);
        }
        
        self.consume(TokenType::Equal, "expected '=' after variable name")?;
        let initializer = self.parse_expression()?;
        self.consume(TokenType::Semicolon, "expected ';' at the end of declaration")?;

        Ok(Stmt::VariableDecl {
            id, is_const, name, type_annotation, initializer: Box::new(initializer)
        })
    }

    fn parse_return_stmt(&mut self) -> Result<Stmt, HydraError> {
        let id = self.next_node_id();
        let mut value = None;

        if !self.check(TokenType::Semicolon) {
            value = Some(Box::new(self.parse_expression()?));
        }

        self.consume(TokenType::Semicolon, "expected ';' after return value")?;

        Ok(Stmt::Return { id, value })
    }

    fn parse_break_stmt(&mut self) -> Result<Stmt, HydraError> {
        let id = self.next_node_id();
        let mut condition = None;

        if self.match_token(TokenType::IF) {
            let has_paren = self.match_token(TokenType::LeftParen);
            condition = Some(Box::new(self.parse_expression()?));
            if has_paren {
                self.consume(TokenType::RightParen, "expected ')' after condition")?;
            }
        }

        self.consume(TokenType::Semicolon, "expected ';' after break")?;
        Ok(Stmt::Break { id, condition })
    }

    fn parse_continue_stmt(&mut self) -> Result<Stmt, HydraError> {
        let id = self.next_node_id();
        let mut condition = None;

        if self.match_token(TokenType::IF) {
            let has_paren = self.match_token(TokenType::LeftParen);
            condition = Some(Box::new(self.parse_expression()?));
            if has_paren {
                self.consume(TokenType::RightParen, "expected ')' after condition")?;
            }
        }

        self.consume(TokenType::Semicolon, "expected ';' after continue")?;
        Ok(Stmt::Continue { id, condition })
    }

    // ========================================================================
    // 5. TYPES
    // ========================================================================

    fn parse_type(&mut self) -> Result<Type, HydraError> {
        if self.match_token(TokenType::LeftBracket) {
            let id = self.next_node_id();
            let start_token = self.previous().clone();
            let element_type = self.parse_type()?;

            // check if it's a slice e.g., `[i32]`
            if self.match_token(TokenType::RightBracket) {
                return Ok(Type::Slice {
                    id,
                    element_type: Box::new(element_type),
                    token: start_token,
                });
            }

            // otherwise it's an array e.g., `[i32, 4]`
            self.consume(TokenType::Comma, "expected ',' to separate type and array size")?;
            let size_expr = self.parse_expression()?;
            self.consume(TokenType::RightBracket, "expected ']' to close the array")?;

            return Ok(Type::Array {
                id,
                element_type: Box::new(element_type),
                size: Box::new(size_expr),
                token: start_token,
            });
        }

        if self.match_token(TokenType::Ampersand) {
            let id = self.next_node_id();
            let is_mut = self.match_token(TokenType::MUT);
            let inner = self.parse_type()?;

            return Ok(Type::Borrow { id, is_mut, inner: Box::new(inner) });
        }

        if self.match_token(TokenType::Star) {
            let id = self.next_node_id();
            let is_mut = if self.match_token(TokenType::MUT) {
                true
            } else if self.match_token(TokenType::CONST) {
                false
            } else {
                return Err(self.error(self.previous(), "P012", "raw pointers must explicitly be '*mut T' or '*const T'"));
            };

            let inner = self.parse_type()?;
            return Ok(Type::RawPointer { id, is_mut, inner: Box::new(inner) });
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
                let mut segments = vec![first_token];

                if self.match_token(TokenType::DoubleColon) {
                    loop {
                        let next_token = self.advance().clone();
                        if let TokenType::IDENTIFIER(_) = next_token.token_type {
                            segments.push(next_token);
                        } else {
                            return Err(self.error(&next_token, "P000", "expected identifier after '::'"));
                        }

                        if !self.match_token(TokenType::DoubleColon) { break; }
                    }
                }

                let mut type_node = Type::Path { id: self.next_node_id(), segments };

                if self.match_token(TokenType::LeftAngle) {
                    let mut args = Vec::new();
                    loop {
                        args.push(self.parse_type()?);
                        if self.match_token(TokenType::RightAngle) { break; }
                        self.consume(TokenType::Comma, "expected ',' between generic types")?;
                    }

                    type_node = Type::Generic { 
                        id: self.next_node_id(), 
                        base: Box::new(type_node), 
                        args 
                    };
                }

                Ok(type_node)
            }

            _ => Err(self.error(current_token, "P000", "expected a type name or array type")
                .with_help("consider adding a type annotation"))
        }
    }

    // ========================================================================
    // 6. GENERICS AND TRAIT BOUNDS
    // ========================================================================

    fn parse_generic_params(&mut self) -> Result<Vec<GenericParam>, HydraError> {
        let mut params = Vec::new();
        if self.match_token(TokenType::LeftAngle) { 
            loop {
                let name = self.consume_identifier("expected generic parameter name")?.clone();
                params.push(GenericParam { id: self.next_node_id(), name });
                
                if self.match_token(TokenType::RightAngle) { break; }
                self.consume(TokenType::Comma, "expected comma between generic parameters")?;
            }
        }
        Ok(params)
    }

    fn parse_where_clause(&mut self) -> Result<Option<WhereClause>, HydraError> {
        if !self.match_token(TokenType::WHERE) {
            return Ok(None);
        }

        let mut predicates = Vec::new();
        loop {
            let target_type = self.parse_type()?;
            self.consume(TokenType::Colon, "expected ':' after type in where clause")?;
            
            let mut bound_traits = Vec::new();
            loop {
                bound_traits.push(self.parse_type()?);
                if !self.match_token(TokenType::Plus) { break; }
            }

            predicates.push(WherePredicate { 
                id: self.next_node_id(), 
                target_type, 
                bound_traits 
            });

            if !self.match_token(TokenType::Comma) { break; }
            if self.check(TokenType::LeftBrace) || self.check(TokenType::Semicolon) { break; }
        }

        Ok(Some(WhereClause { predicates }))
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

                        if !self.match_token(TokenType::Comma) { break; }
                    }
                }
                self.consume(TokenType::RightParen, "expected ')' after annotation arguments")?;
            }
            
            self.consume(TokenType::RightBracket, "expected ']' to close attribute")?;
            annotations.push(Annotation { name, args });
        }

        Ok(annotations)
    }

    // ========================================================================
    // 7. EXPRESSIONS
    // ========================================================================

    fn parse_expression(&mut self) -> Result<Expr, HydraError> {
        self.parse_assignment()
    }

    fn parse_assignment(&mut self) -> Result<Expr, HydraError> {
        let target = self.parse_logical_or()?; 

        if self.match_token(TokenType::Equal) ||
            self.match_token(TokenType::PlusEqual) ||
            self.match_token(TokenType::MinusEqual) ||
            self.match_token(TokenType::StarEqual) ||
            self.match_token(TokenType::ForwardSlashEqual) ||
            self.match_token(TokenType::ModuloEqual)
        {
            let id = self.next_node_id();
            let operator = self.previous().clone();
            let value = self.parse_assignment()?;
            
            return Ok(Expr::Assignment {
                id,
                target: Box::new(target),
                operator,
                value: Box::new(value)
            });
        }

        Ok(target)
    }

    fn parse_logical_or(&mut self) -> Result<Expr, HydraError> {
        let mut node = self.parse_logical_and()?;

        while self.match_token(TokenType::DoublePipe) {
            let id = self.next_node_id();
            let operator = self.previous().clone();
            let right = self.parse_logical_and()?;

            node = Expr::Binary {
                id, left: Box::new(node), operator, right: Box::new(right),
            };
        }

        Ok(node)
    }

    fn parse_logical_and(&mut self) -> Result<Expr, HydraError> {
        let mut node = self.parse_equality()?;

        while self.match_token(TokenType::DoubleAmpersand) {
            let id = self.next_node_id();
            let operator = self.previous().clone();
            let right = self.parse_equality()?;

            node = Expr::Binary {
                id, left: Box::new(node), operator, right: Box::new(right),
            };
        }

        Ok(node)
    }

    fn parse_equality(&mut self) -> Result<Expr, HydraError> {
        let mut node = self.parse_comparison()?;

        while self.match_token(TokenType::DoubleEqual) || self.match_token(TokenType::ExclamEqual) {
            let id = self.next_node_id();
            let operator = self.previous().clone();
            let right = self.parse_comparison()?;

            node = Expr::Binary {
                id, left: Box::new(node), operator, right: Box::new(right),
            };
        }

        Ok(node)
    }

    fn parse_comparison(&mut self) -> Result<Expr, HydraError> {
        let mut node = self.parse_additive()?;

        while self.match_token(TokenType::LeftAngle) || self.match_token(TokenType::LessEqual) ||
            self.match_token(TokenType::RightAngle) || self.match_token(TokenType::GreaterEqual)
        {
            let id = self.next_node_id();
            let operator = self.previous().clone();
            let right = self.parse_additive()?;

            node = Expr::Binary { 
                id, left: Box::new(node), operator, right: Box::new(right)
            };
        }

        Ok(node)
    }

    fn parse_additive(&mut self) -> Result<Expr, HydraError> {
        let mut node = self.parse_multiplicative()?;

        loop {
            let operator = if self.match_token(TokenType::Plus) || self.match_token(TokenType::Minus) {
                Some(self.previous().clone())
            } else {
                None
            };

            if let Some(op) = operator {
                let id = self.next_node_id();
                let right = self.parse_multiplicative()?;
                node = Expr::Binary {
                    id, left: Box::new(node), operator: op, right: Box::new(right)
                };
            } else {
                break;
            }
        }

        Ok(node)
    }

    fn parse_multiplicative(&mut self) -> Result<Expr, HydraError> {
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
                let id = self.next_node_id();
                let right = self.parse_unary()?;
                node = Expr::Binary {
                    id, left: Box::new(node), operator: op, right: Box::new(right),
                };
            } else {
                break;
            }
        }

        Ok(node)
    }

    fn parse_unary(&mut self) -> Result<Expr, HydraError> {
        // borrowing
        if self.match_token(TokenType::Ampersand) {
            let id = self.next_node_id();
            let is_mut = self.match_token(TokenType::MUT);
            let right = self.parse_unary()?;

            return Ok(Expr::Borrow {
                id, is_mut, right: Box::new(right),
            });
        }

        // dereference
        if self.match_token(TokenType::Star) {
            let id = self.next_node_id();
            let right = self.parse_unary()?;

            return Ok(Expr::Dereference {
                id, right: Box::new(right)
            });
        }

        // unary ops
        if self.match_token(TokenType::ExclamationMark) || self.match_token(TokenType::Minus) {
            let id = self.next_node_id();
            let operator = self.previous().clone();
            let right = self.parse_unary()?;

            return Ok(Expr::Unary {
                id, operator, right: Box::new(right),
            });
        }

        self.parse_call()
    }

    fn parse_call(&mut self) -> Result<Expr, HydraError> {
        let mut expr = self.parse_primary()?;
        
        loop {
            if self.match_token(TokenType::LeftParen) {
                let id = self.next_node_id();
                let arguments = self.finish_parse_fn_call_args()?;
                expr = Expr::FunctionCall { id, callee: Box::new(expr), arguments, generic_args: Vec::new() };
            } else if self.match_token(TokenType::AS) {
                let id = self.next_node_id();
                let target = self.parse_type()?;
                expr = Expr::Cast { id, value: Box::new(expr), target: Box::new(target) };
            } else if self.match_token(TokenType::DoubleColon) {
                if self.match_token(TokenType::LeftAngle) {
                    let id = self.next_node_id();
                    let mut generic_args = Vec::new();
                    loop {
                        generic_args.push(self.parse_type()?);
                        if self.match_token(TokenType::RightAngle) { break; }
                        self.consume(TokenType::Comma, "expected ',' in generic args")?;
                    }

                    self.consume(TokenType::LeftParen, "expected '(' after generic arguments")?;
                    let arguments = self.finish_parse_fn_call_args()?;
                    expr = Expr::FunctionCall { id, callee: Box::new(expr), arguments, generic_args };
                } else {
                    let next_name = self.consume_identifier("expected identifier after '::'")?.clone();

                    expr = match expr {
                        Expr::Variable { name, .. } => {
                            Expr::Path { id: self.next_node_id(), segments: vec![name, next_name] }
                        },
                        Expr::Path { mut segments, .. } => {
                            segments.push(next_name);
                            Expr::Path { id: self.next_node_id(), segments }
                        },
                        other => {
                            Expr::Member { id: self.next_node_id(), object: Box::new(other), property: next_name }
                        }
                    };
                }
            } else if self.allow_struct && self.check(TokenType::LeftBrace) {
                expr = self.parse_struct_initializer(expr)?;
            } else if self.match_token(TokenType::Dot) {
                let id = self.next_node_id();
                let name = if let TokenType::IDENTIFIER(_) = self.peek().token_type {
                    self.advance().clone()
                } else {
                    return Err(self.error(self.peek(), "P001", "expected property name after '.'"));
                };

                expr = Expr::Member { id, object: Box::new(expr), property: name };
            } else if self.match_token(TokenType::DoubleColon) {
                if self.match_token(TokenType::LeftAngle) {
                    let id = self.next_node_id();
                    let mut generic_args = Vec::new();
                    loop {
                        generic_args.push(self.parse_type()?);
                        if self.match_token(TokenType::RightAngle) { break; }
                        self.consume(TokenType::Comma, "expected ',' in generic args")?;
                    }
                    self.consume(TokenType::LeftParen, "expected '(' after generic arguments")?;
                    let arguments = self.finish_parse_fn_call_args()?;
                    expr = Expr::FunctionCall { id, callee: Box::new(expr), arguments, generic_args };
                } else {
                    let next_name = self.consume_identifier("expected identifier after '::'")?.clone();

                    expr = match expr {
                        Expr::Variable { name, .. } => {
                            Expr::Path { id: self.next_node_id(), segments: vec![name, next_name] }
                        },
                        Expr::Path { mut segments, .. } => {
                            segments.push(next_name);
                            Expr::Path { id: self.next_node_id(), segments }
                        },
                        other => {
                            let mut generic_args = Vec::new();
                            if self.match_token(TokenType::DoubleColon) {
                                self.consume(TokenType::LeftAngle, "expected '<' after '::' for method generics")?;
                                loop {
                                    generic_args.push(self.parse_type()?);
                                    if self.match_token(TokenType::RightAngle) { break; }
                                    self.consume(TokenType::Comma, "expected ',' in generic args")?;
                                }
                            }
                            if self.match_token(TokenType::LeftParen) {
                                let arguments = self.finish_parse_fn_call_args()?;
                                Expr::MethodCall { id: self.next_node_id(), object: Box::new(other), method: next_name, arguments, generic_args }
                            } else {
                                Expr::Member { id: self.next_node_id(), object: Box::new(other), property: next_name }
                            }
                        }
                    }
                }
            } else if self.match_token(TokenType::PlusPlus) || self.match_token(TokenType::MinusMinus) {
                let id = self.next_node_id();
                expr = Expr::PostfixUnary { id, operator: self.previous().clone(), left: Box::new(expr) };
            } else if self.match_token(TokenType::LeftBracket) {
                let id = self.next_node_id();
                let token = self.previous().clone();
                let index = self.parse_expression()?;
                self.consume(TokenType::RightBracket, "expected ']' after array index")?;
                expr = Expr::ArrayAccess { id, array: Box::new(expr), index: Box::new(index), token };
            } else {
                break;
            }
        }
        
        Ok(expr)
    }

    fn finish_parse_fn_call_args(&mut self) -> Result<Vec<Expr>, HydraError> {
        let mut args = Vec::new();

        if !self.check(TokenType::RightParen) {
            loop {
                args.push(self.parse_expression()?);
                if !self.match_token(TokenType::Comma) { break; }
            }
        }

        self.consume(TokenType::RightParen, "expected ')' after arguments")?;
        Ok(args)
    }

    fn parse_struct_initializer(&mut self, name_expr: Expr) -> Result<Expr, HydraError> {
        let id = self.next_node_id();
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

        Ok(Expr::StructInitializer { id, name: Box::new(name_expr), fields })
    }

    fn parse_primary(&mut self) -> Result<Expr, HydraError> {
        // check for control flow expressions first 
        if self.match_token(TokenType::IF) { return self.parse_if(); }
        if self.match_token(TokenType::WHILE) { return self.parse_while(); }
        if self.match_token(TokenType::FOR) { return self.parse_for(); }
        if self.match_token(TokenType::FOREACH) { return self.parse_foreach(); }

        let id = self.next_node_id();
        let current_token = &self.tokens[self.current];

        use TokenType::*;
        match &current_token.token_type {
            IntLiteral(_) | FloatLiteral(_) | StringLiteral(_) | CharLiteral(_)| BoolLiteral(_) | ANYSIZE => {
                self.advance();
                Ok(Expr::Literal { id, token: self.previous().clone() })
            }

            IDENTIFIER(_) => {
                let name_token = self.advance().clone();
                let path_node = Expr::Variable { id, name: name_token };

                // this safely falls through if we're not allowing struct initializers
                if self.allow_struct && self.check(TokenType::LeftBrace) {
                    return self.parse_struct_initializer(path_node);
                }

                Ok(path_node)
            }

            LeftParen => {
                self.advance();
                let expr = self.parse_expression()?; 
                self.consume(TokenType::RightParen, "expected ')' after expression")?;
                Ok(expr)
            }

            LeftBrace => self.parse_array_initializer(),

            Star | ForwardSlash | Plus | Modulo => {
                Err(self.error(current_token, "P004", format!("unexpected operator `{}` found here", current_token.lexeme)))
            },

            // these act as path bases for static methods or constraints
            ISIZE | I8 | I16 | I32 | I64 | USIZE | U8 | U16 | U32 | U64 | F32 | F64 | CHAR | BOOL => {
                let token = self.advance().clone();
                Ok(Expr::Variable { id, name: token })
            }

            _ => Err(self.error(current_token, "P000", "expected a value (number, string, or boolean)"))
        }
    }

    fn parse_array_initializer(&mut self) -> Result<Expr, HydraError> {
        let id = self.next_node_id();
        let start_token = self.consume(TokenType::LeftBrace, "expected '{' to start array initializer")?.clone();
        let mut elements = Vec::new();

        if !self.check(TokenType::RightBrace) {
            loop {
                elements.push(self.parse_expression()?);
                if !self.match_token(TokenType::Comma) { break; }
                if self.check(TokenType::RightBrace) { break; }
            }
        }
        self.consume(TokenType::RightBrace, "expected '}' to close array initializer")?;

        Ok(Expr::ArrayInitializer { id, elements, token: start_token })
    }

    fn parse_if(&mut self) -> Result<Expr, HydraError> {
        let id = self.next_node_id();
        let has_paren = self.match_token(TokenType::LeftParen);

        if !has_paren { self.allow_struct = false; }
        let condition = self.parse_expression()?;
        if !has_paren { self.allow_struct = true; }

        if has_paren {
            self.consume(TokenType::RightParen, "expected ')' after if condition")?;
        }

        let then_branch = self.parse_block()?;

        let else_branch = if self.match_token(TokenType::ELSE) {
            if self.match_token(TokenType::IF) {
                // to support `else if`, wrap nested if inside a block
                let nested_if_id = self.next_node_id();
                let nested_if = self.parse_if()?;

                Some(Block {
                    id: nested_if_id,
                    statements: vec![Stmt::Expr(Box::new(nested_if))]
                })
            } else {
                Some(self.parse_block()?)
            }
        } else {
            None
        };

        Ok(Expr::If { id, condition: Box::new(condition), then_branch, else_branch })
    }

    fn parse_while(&mut self) -> Result<Expr, HydraError> {
        let id = self.next_node_id();
        let has_paren = self.match_token(TokenType::LeftParen);        
        
        if !has_paren { self.allow_struct = false; }
        let condition = self.parse_expression()?;
        if !has_paren { self.allow_struct = true; }

        if has_paren {
            self.consume(TokenType::RightParen, "expected ')' after while condition")?;
        }

        let body = self.parse_block()?;

        Ok(Expr::While { id, condition: Box::new(condition), body })
    }

    fn parse_for(&mut self) -> Result<Expr, HydraError> {
        let id = self.next_node_id();
        let has_paren = self.match_token(TokenType::LeftParen);

        if !has_paren { self.allow_struct = false; }
        let variable = self.consume(TokenType::IDENTIFIER("".to_string()), "expected loop variable name")?.clone();
        self.consume(TokenType::IN, "expected 'in' after loop variable")?;

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
            self.consume(TokenType::RightParen, "expected ')' after range")?;
        }

        let body = self.parse_block()?;

        Ok(Expr::For { id, variable, start: Box::new(start), end: Box::new(end), is_inclusive, body })
    }
    
    fn parse_foreach(&mut self) -> Result<Expr, HydraError> {
        let id = self.next_node_id();
        let has_paren = self.match_token(TokenType::LeftParen);

        if !has_paren { self.allow_struct = false; }

        let item_name = self.consume(TokenType::IDENTIFIER("".to_string()), "expected item name")?.clone();
        self.consume(TokenType::IN, "expected 'in' after item name")?;

        let iterable = self.parse_expression()?;

        if !has_paren { self.allow_struct = true; }

        if has_paren {
            self.consume(TokenType::RightParen, "expected ')' after iterable")?;
        }

        let body = self.parse_block()?;

        Ok(Expr::ForEach { id, item: item_name, iterable: Box::new(iterable), body })
    }

    // ========================================================================
    // 8. HELPERS
    // ========================================================================

    fn match_token(&mut self, token: TokenType) -> bool {
        if self.check(token) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn consume(&mut self, token: TokenType, expected: &str) -> Result<&Token, HydraError> {
        if self.check(token) {
            Ok(self.advance())
        } else {
            Err(self.error(self.peek(), "P002", format!("expected {}, but found `{}`", expected, self.peek().lexeme)))
        }
    }

    fn consume_identifier(&mut self, expected: &str) -> Result<Token, HydraError> {
        if let TokenType::IDENTIFIER(_) = self.peek().token_type {
            Ok(self.advance().clone())
        } else {
            Err(self.error(self.peek(), "P002", format!("expected {}, found '{}'", expected, self.peek().lexeme)))
        }
    }

    fn check_at(&self, offset: usize, token_type: TokenType) -> bool {
        let pos = self.current + offset;

        if pos >= self.tokens.len() || self.tokens[pos].token_type == TokenType::EOF {
            return false;
        }

        std::mem::discriminant(&self.tokens[pos].token_type) == std::mem::discriminant(&token_type)
    }

    fn peek(&self) -> &Token {
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

    fn skip_block(&mut self) -> Result<(), HydraError> {
        self.consume(TokenType::LeftBrace, "expected '{' to start block")?;
        let mut depth = 1;

        while depth > 0 && !self.is_at_end() {
            if self.check(TokenType::LeftBrace) {
                depth += 1;
            } else if self.check(TokenType::RightBrace) {
                depth -= 1;
            }
            self.advance();
        }

        if depth > 0 {
            return Err(self.error(self.peek(), "P014", "unterminated block"));
        }
        Ok(())
    }

    fn advance(&mut self) -> &Token {
        if !self.is_at_end() {
            self.current += 1;
        }
        self.previous()
    }

    fn is_at_end(&self) -> bool {
        self.tokens[self.current].token_type == TokenType::EOF
    }

    fn previous(&self) -> &Token {
        &self.tokens[self.current - 1]
    }
}
