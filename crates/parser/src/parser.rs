// This file implements a recursive descent parser for the Hydra programming language

use lexer::{Token, TokenType};
use crate::ast::*;

pub struct Parser<'a> {
    tokens: Vec<Token<'a>>,
    current: usize,
}

impl<'a> Parser<'a> {
    pub fn new(tokens: Vec<Token<'a>>) -> Self {
        Self {
            tokens,
            current: 0,
        }
    }

    pub fn parse(&mut self) -> Result<Program, String> {
        let mut statements = Vec::new();

        while !self.is_at_end() {
            // Skip newlines at the top level
            if self.match_token(&TokenType::Newline) {
                continue;
            }
            statements.push(self.parse_statement()?);
        }

        Ok(statements)
    }

    //------------------------------------------------------------------
    // ## Utility methods
    //------------------------------------------------------------------
    fn peek(&self) -> &TokenType {
        &self.tokens[self.current].token_type
    }

    fn previous(&self) -> &TokenType {
        &self.tokens[self.current - 1].token_type
    }

    fn is_at_end(&self) -> bool {
        *self.peek() == TokenType::Eof
    }

    fn advance(&mut self) -> &TokenType {
        if !self.is_at_end() {
            self.current += 1;
        }
        &self.tokens[self.current - 1].token_type
    }

    fn match_token(&mut self, token_type: &TokenType) -> bool {
        if self.check(token_type) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn check(&self, token_type: &TokenType) -> bool {
        if self.is_at_end() {
            false
        } else {
            std::mem::discriminant(self.peek()) == std::mem::discriminant(token_type)
        }
    }

    fn consume(&mut self, token_type: &TokenType, message: &str) -> Result<(), String> {
        if self.check(token_type) {
            self.advance();
            Ok(())
        } else {
            Err(format!("{} at line {}, column {}", message, 
                self.tokens[self.current].line, 
                self.tokens[self.current].column))
        }
    }

    fn get_identifier(&self) -> Result<String, String> {
        match self.previous() {
            TokenType::Identifier(name) => Ok(name.clone()),
            _ => Err("Expected identifier".to_string()),
        }
    }
    
    fn compound_assign_to_binary_op(&self, token_type: &TokenType) -> Result<BinaryOp, String> {
        match token_type {
            TokenType::PlusAssign => Ok(BinaryOp::Add),
            TokenType::MinusAssign => Ok(BinaryOp::Subtract),
            TokenType::MultiplyAssign => Ok(BinaryOp::Multiply),
            TokenType::DivideAssign => Ok(BinaryOp::Divide),
            TokenType::ModuloAssign => Ok(BinaryOp::Modulo),
            _ => Err("Invalid compound assignment operator".to_string()),
        }
    }

    //------------------------------------------------------------------
    // ## Statement parsing
    //------------------------------------------------------------------
    fn parse_statement(&mut self) -> Result<Statement, String> {
        match self.peek() {
            TokenType::Let | TokenType::Const => self.parse_variable_declaration(),
            TokenType::Function => self.parse_function_declaration(),
            TokenType::Struct => self.parse_struct_declaration(),
            TokenType::Include => self.parse_include_declaration(),
            TokenType::Typedef => self.parse_typedef_declaration(),
            TokenType::If => self.parse_if_statement(),
            TokenType::For => self.parse_for_statement(),
            TokenType::ForEach => self.parse_foreach_statement(),
            TokenType::While => self.parse_while_statement(),
            TokenType::Match => self.parse_match_statement(),
            TokenType::Return => self.parse_return_statement(),
            TokenType::Break => {
                self.advance();
                self.consume(&TokenType::Semicolon, "Expected ';' after 'break'")?;
                Ok(Statement::Break)
            },
            TokenType::Skip => {
                self.advance();
                self.consume(&TokenType::Semicolon, "Expected ';' after 'skip'")?;
                Ok(Statement::Skip)
            },
            TokenType::LeftBrace => self.parse_block_statement(),
            _ => self.parse_expression_statement(),
        }
    }

    fn parse_variable_declaration(&mut self) -> Result<Statement, String> {
        let is_mutable = match self.advance() {
            TokenType::Let => true,
            TokenType::Const => false,
            _ => return Err("Expected 'let' or 'const'".to_string()),
        };

        if !self.match_token(&TokenType::Identifier(String::new())) {
            return Err("Expected variable name".to_string());
        }
        let name = self.get_identifier()?;

        self.consume(&TokenType::Colon, "Expected ':' after variable name")?;
        let var_type = self.parse_type()?;

        self.consume(&TokenType::Assign, "Expected '=' after variable type")?;
        let initializer = self.parse_expression()?;

        self.consume(&TokenType::Semicolon, "Expected ';' after variable declaration")?;

        Ok(Statement::VariableDeclaration(VarDecl {
            is_mutable,
            name,
            var_type,
            initializer,
        }))
    }

    fn parse_function_declaration(&mut self) -> Result<Statement, String> {
        self.advance(); // consume 'fn'

        if !self.match_token(&TokenType::Identifier(String::new())) {
            return Err("Expected function name".to_string());
        }
        let name = self.get_identifier()?;

        self.consume(&TokenType::LeftParen, "Expected '(' after function name")?;

        let mut params = Vec::new();
        if !self.check(&TokenType::RightParen) {
            loop {
                if !self.match_token(&TokenType::Identifier(String::new())) {
                    return Err("Expected parameter name".to_string());
                }
                let param_name = self.get_identifier()?;

                self.consume(&TokenType::Colon, "Expected ':' after parameter name")?;
                let param_type = self.parse_type()?;

                params.push(Parameter {
                    name: param_name,
                    param_type,
                });

                if !self.match_token(&TokenType::Comma) {
                    break;
                }
            }
        }

        self.consume(&TokenType::RightParen, "Expected ')' after parameters")?;
        self.consume(&TokenType::Arrow, "Expected '->' after parameters")?;

        let return_type = self.parse_type()?;

        self.consume(&TokenType::LeftBrace, "Expected '{' before function body")?;

        let mut body = Vec::new();
        while !self.check(&TokenType::RightBrace) && !self.is_at_end() {
            if self.match_token(&TokenType::Newline) {
                continue;
            }
            body.push(self.parse_statement()?);
        }

        self.consume(&TokenType::RightBrace, "Expected '}' after function body")?;

        Ok(Statement::FunctionDeclaration(FnDecl {
            name,
            params,
            return_type,
            body,
        }))
    }

    fn parse_struct_declaration(&mut self) -> Result<Statement, String> {
        self.advance(); // consume 'struct'

        if !self.match_token(&TokenType::Identifier(String::new())) {
            return Err("Expected struct name".to_string());
        }
        let name = self.get_identifier()?;

        self.consume(&TokenType::LeftBrace, "Expected '{' after struct name")?;

        let mut fields = Vec::new();
        let mut methods = Vec::new();

        while !self.check(&TokenType::RightBrace) && !self.is_at_end() {
            if self.match_token(&TokenType::Newline) {
                continue;
            }

            if self.check(&TokenType::Function) {
                // Parse method
                match self.parse_function_declaration()? {
                    Statement::FunctionDeclaration(fn_decl) => methods.push(fn_decl),
                    _ => unreachable!(),
                }
            } else {
                // Parse field
                if !self.match_token(&TokenType::Identifier(String::new())) {
                    return Err("Expected field name".to_string());
                }
                let field_name = self.get_identifier()?;

                self.consume(&TokenType::Colon, "Expected ':' after field name")?;
                let field_type = self.parse_type()?;

                self.consume(&TokenType::Comma, "Expected ',' after field declaration")?;

                fields.push(StructField {
                    name: field_name,
                    field_type,
                });
            }
        }

        self.consume(&TokenType::RightBrace, "Expected '}' after struct body")?;

        Ok(Statement::StructDeclaration(StructDecl {
            name,
            fields,
            methods,
        }))
    }

    fn parse_include_declaration(&mut self) -> Result<Statement, String> {
        self.advance(); // consume 'include'

        let path = match self.advance() {
            TokenType::StringLiteral(s) => s.clone(),
            _ => return Err("Expected string literal after 'include'".to_string()),
        };

        self.consume(&TokenType::Semicolon, "Expected ';' after include path")?;

        Ok(Statement::IncludeDeclaration(IncludeDecl { path }))
    }

    fn parse_typedef_declaration(&mut self) -> Result<Statement, String> {
        self.advance(); // consume 'typedef'

        if !self.match_token(&TokenType::Identifier(String::new())) {
            return Err("Expected type alias name".to_string());
        }
        let alias = self.get_identifier()?;

        self.consume(&TokenType::Assign, "Expected '=' after type alias")?;
        let original_type = self.parse_type()?;

        self.consume(&TokenType::Semicolon, "Expected ';' after typedef")?;

        Ok(Statement::TypedefDeclaration(TypedefDecl {
            alias,
            original_type,
        }))
    }

    fn parse_if_statement(&mut self) -> Result<Statement, String> {
        self.advance(); // consume 'if'

        self.consume(&TokenType::LeftParen, "Expected '(' after 'if'")?;
        let condition = self.parse_expression()?;
        self.consume(&TokenType::RightParen, "Expected ')' after if condition")?;

        let then_branch = Box::new(self.parse_statement()?);

        let else_branch = if self.match_token(&TokenType::Else) {
            Some(Box::new(self.parse_statement()?))
        } else {
            None
        };

        Ok(Statement::If {
            condition,
            then_branch,
            else_branch,
        })
    }
    
    fn parse_match_statement(&mut self) -> Result<Statement, String> {
        self.advance(); // consume 'match'
        let expr = self.parse_expression()?;
        self.consume(&TokenType::LeftBrace, "Expected '{' after match expression")?;

        let mut arms = Vec::new();
        while !self.check(&TokenType::RightBrace) && !self.is_at_end() {
            if self.match_token(&TokenType::Newline) {
                continue;
            }
            arms.push(self.parse_match_arm()?);
        }

        self.consume(&TokenType::RightBrace, "Expected '}' after match arms")?;
        Ok(Statement::Match { expr, arms })
    }

    fn parse_match_arm(&mut self) -> Result<MatchArm, String> {
        let pattern = self.parse_pattern()?;
        self.consume(&TokenType::EqualArrow, "Expected '=>' after match pattern")?;
        let value = self.parse_expression()?;

        // Arms can be separated by commas, and the last one might not have one.
        self.match_token(&TokenType::Comma);

        Ok(MatchArm { pattern, value })
    }

    fn parse_pattern(&mut self) -> Result<Pattern, String> {
        match self.peek() {
            TokenType::IntLiteral(_) | TokenType::FloatLiteral(_) |
            TokenType::StringLiteral(_) | TokenType::CharLiteral(_) |
            TokenType::BoolLiteral(true) | TokenType::BoolLiteral(false) | TokenType::None => {
                match self.parse_primary()? {
                    Expr::Literal(lit) => Ok(Pattern::Literal(lit)),
                    _ => Err("Expected a literal pattern.".to_string())
                }
            },
            TokenType::Identifier(_) => {
                if let TokenType::Identifier(name) = self.advance().clone() {
                    if name == "_" {
                        Ok(Pattern::Wildcard)
                    } else {
                        Ok(Pattern::Identifier(name))
                    }
                } else { unreachable!() }
            }
            _ => Err(format!("Unexpected token in pattern: {:?}", self.peek())),
        }
    }

    fn parse_for_statement(&mut self) -> Result<Statement, String> {
        self.advance(); // consume 'for'

        self.consume(&TokenType::LeftParen, "Expected '(' after 'for'")?;

        if !self.match_token(&TokenType::Identifier(String::new())) {
            return Err("Expected iterator variable name".to_string());
        }
        let iterator_name = self.get_identifier()?;

        self.consume(&TokenType::In, "Expected 'in' after iterator variable")?;

        let range = self.parse_expression()?;

        self.consume(&TokenType::RightParen, "Expected ')' after for range")?;

        self.consume(&TokenType::LeftBrace, "Expected '{' after for header")?;

        let mut body = Vec::new();
        while !self.check(&TokenType::RightBrace) && !self.is_at_end() {
            if self.match_token(&TokenType::Newline) {
                continue;
            }
            body.push(self.parse_statement()?);
        }

        self.consume(&TokenType::RightBrace, "Expected '}' after for body")?;

        Ok(Statement::For(ForRangeLoop {
            iterator_name,
            range,
            body,
        }))
    }

    fn parse_foreach_statement(&mut self) -> Result<Statement, String> {
        self.advance(); // consume 'forEach'

        self.consume(&TokenType::LeftParen, "Expected '(' after 'forEach'")?;

        if !self.match_token(&TokenType::Identifier(String::new())) {
            return Err("Expected element variable name".to_string());
        }
        let element_name = self.get_identifier()?;

        self.consume(&TokenType::In, "Expected 'in' after element variable")?;

        let iterable = self.parse_expression()?;

        self.consume(&TokenType::RightParen, "Expected ')' after forEach iterable")?;

        self.consume(&TokenType::LeftBrace, "Expected '{' after forEach header")?;

        let mut body = Vec::new();
        while !self.check(&TokenType::RightBrace) && !self.is_at_end() {
            if self.match_token(&TokenType::Newline) {
                continue;
            }
            body.push(self.parse_statement()?);
        }

        self.consume(&TokenType::RightBrace, "Expected '}' after forEach body")?;

        Ok(Statement::ForEach(ForEachLoop {
            element_name,
            iterable,
            body,
        }))
    }

    fn parse_while_statement(&mut self) -> Result<Statement, String> {
        self.advance(); // consume 'while'

        self.consume(&TokenType::LeftParen, "Expected '(' after 'while'")?;
        let condition = self.parse_expression()?;
        self.consume(&TokenType::RightParen, "Expected ')' after while condition")?;

        self.consume(&TokenType::LeftBrace, "Expected '{' after while condition")?;

        let mut body = Vec::new();
        while !self.check(&TokenType::RightBrace) && !self.is_at_end() {
            if self.match_token(&TokenType::Newline) {
                continue;
            }
            body.push(self.parse_statement()?);
        }

        self.consume(&TokenType::RightBrace, "Expected '}' after while body")?;

        Ok(Statement::While { condition, body })
    }

    fn parse_return_statement(&mut self) -> Result<Statement, String> {
        self.advance(); // consume 'return'

        let value = if self.check(&TokenType::Semicolon) {
            None
        } else {
            Some(self.parse_expression()?)
        };

        self.consume(&TokenType::Semicolon, "Expected ';' after return value")?;

        Ok(Statement::Return(value))
    }

    fn parse_block_statement(&mut self) -> Result<Statement, String> {
        self.consume(&TokenType::LeftBrace, "Expected '{'")?;

        let mut statements = Vec::new();
        while !self.check(&TokenType::RightBrace) && !self.is_at_end() {
            if self.match_token(&TokenType::Newline) {
                continue;
            }
            statements.push(self.parse_statement()?);
        }

        self.consume(&TokenType::RightBrace, "Expected '}'")?;

        Ok(Statement::Block(statements))
    }

    fn parse_expression_statement(&mut self) -> Result<Statement, String> {
        let expr = self.parse_expression()?;
        self.consume(&TokenType::Semicolon, "Expected ';' after expression")?;
        Ok(Statement::Expression(expr))
    }

    //------------------------------------------------------------------
    // ## Type parsing
    //------------------------------------------------------------------
    fn parse_type(&mut self) -> Result<Type, String> {
        if self.match_token(&TokenType::LeftBracket) {
            // Array type: [const? type, size]
            let element_const = self.match_token(&TokenType::Const);
            
            let element_type = Box::new(self.parse_type()?);
            
            self.consume(&TokenType::Comma, "Expected ',' after array element type")?;
            
            let size = match self.advance() {
                TokenType::IntLiteral(n) => ArraySize::Concrete(*n as u64),
                TokenType::Identifier(name) => ArraySize::Generic(name.clone()),
                _ => return Err("Expected array size".to_string()),
            };

            self.consume(&TokenType::RightBracket, "Expected ']' after array size")?;

            Ok(Type::Array {
                element_const,
                element_type,
                size,
            })
        } else if self.match_token(&TokenType::Identifier(String::new())) {
            let type_name = self.get_identifier()?;
            Ok(match type_name.as_str() {
                "i8" => Type::I8,
                "i16" => Type::I16,
                "i32" => Type::I32,
                "i64" => Type::I64,
                "u8" => Type::U8,
                "u16" => Type::U16,
                "u32" => Type::U32,
                "u64" => Type::U64,
                "f32" => Type::F32,
                "f64" => Type::F64,
                "char" => Type::Char,
                "bool" => Type::Bool,
                "string" => Type::String,
                "void" => Type::Void,
                _ => Type::Custom(type_name),
            })
        } else {
            Err("Expected type".to_string())
        }
    }

    //------------------------------------------------------------------
    // ## Expression parsing (Precedence Climbing)
    //------------------------------------------------------------------
    // The functions are ordered from lowest to highest precedence.
    // parse_expression -> assignment -> or -> and -> bitwise_or -> ... -> primary
    //------------------------------------------------------------------

    fn parse_expression(&mut self) -> Result<Expr, String> {
        self.parse_assignment()
    }

    fn parse_assignment(&mut self) -> Result<Expr, String> {
        let expr = self.parse_or()?;

        // Assignment is right-associative and has lower precedence than almost everything.
        // It's not a standard binary operator, so it's handled specially here.
        if self.match_token(&TokenType::Assign) {
            let value = self.parse_assignment()?; // Recursive call for right-associativity
            
            match expr {
                Expr::Variable(name) => Ok(Expr::Assignment { name, value: Box::new(value) }),
                // TODO: Add support for assigning to fields, indexes, etc.
                _ => Err("Invalid assignment target.".to_string())
            }
        } else if self.check(&TokenType::PlusAssign) ||
                  self.check(&TokenType::MinusAssign) ||
                  self.check(&TokenType::MultiplyAssign) ||
                  self.check(&TokenType::DivideAssign) ||
                  self.check(&TokenType::ModuloAssign) {

            let op = self.compound_assign_to_binary_op(self.advance())?;
            let value = self.parse_assignment()?;

            match expr {
                Expr::Variable(name) => Ok(Expr::CompoundAssignment { name, op, value: Box::new(value) }),
                _ => Err("Invalid compound assignment target.".to_string())
            }
        } else {
            Ok(expr)
        }
    }

    fn parse_or(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_and()?;

        while self.match_token(&TokenType::Or) {
            let right = Box::new(self.parse_and()?);
            expr = Expr::Binary {
                left: Box::new(expr),
                op: BinaryOp::Or,
                right,
            };
        }

        Ok(expr)
    }

    fn parse_and(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_bitwise_or()?;

        while self.match_token(&TokenType::And) {
            let right = Box::new(self.parse_bitwise_or()?);
            expr = Expr::Binary {
                left: Box::new(expr),
                op: BinaryOp::And,
                right,
            };
        }

        Ok(expr)
    }
    
    fn parse_bitwise_or(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_bitwise_xor()?;
        while self.match_token(&TokenType::BitwiseOr) {
            let right = Box::new(self.parse_bitwise_xor()?);
            expr = Expr::Binary {
                left: Box::new(expr),
                op: BinaryOp::BitwiseOr,
                right,
            };
        }
        Ok(expr)
    }

    fn parse_bitwise_xor(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_bitwise_and()?;
        while self.match_token(&TokenType::BitwiseXor) {
            let right = Box::new(self.parse_bitwise_and()?);
            expr = Expr::Binary {
                left: Box::new(expr),
                op: BinaryOp::BitwiseXor,
                right,
            };
        }
        Ok(expr)
    }

    fn parse_bitwise_and(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_equality()?;
        while self.match_token(&TokenType::BitwiseAnd) {
            let right = Box::new(self.parse_equality()?);
            expr = Expr::Binary {
                left: Box::new(expr),
                op: BinaryOp::BitwiseAnd,
                right,
            };
        }
        Ok(expr)
    }


    fn parse_equality(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_comparison()?;

        while self.check(&TokenType::Equal) || self.check(&TokenType::NotEqual) {
            let op = match self.advance() {
                TokenType::Equal => BinaryOp::Equal,
                TokenType::NotEqual => BinaryOp::NotEqual,
                _ => unreachable!(),
            };
            let right = Box::new(self.parse_comparison()?);
            expr = Expr::Binary {
                left: Box::new(expr),
                op,
                right,
            };
        }

        Ok(expr)
    }

    fn parse_comparison(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_range()?;

        while self.check(&TokenType::LeftAngle) || 
              self.check(&TokenType::RightAngle) ||
              self.check(&TokenType::LessEqual) ||
              self.check(&TokenType::GreaterEqual) {
            let op = match self.advance() {
                TokenType::LeftAngle => BinaryOp::Less,
                TokenType::RightAngle => BinaryOp::Greater,
                TokenType::LessEqual => BinaryOp::LessEqual,
                TokenType::GreaterEqual => BinaryOp::GreaterEqual,
                _ => unreachable!(),
            };
            let right = Box::new(self.parse_range()?);
            expr = Expr::Binary {
                left: Box::new(expr),
                op,
                right,
            };
        }

        Ok(expr)
    }

    fn parse_range(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_bitshift()?;

        if self.check(&TokenType::RangeExclusive) || self.check(&TokenType::RangeInclusive) {
            let is_inclusive = self.match_token(&TokenType::RangeInclusive);
            if !is_inclusive {
                self.advance(); // consume '..'
            }
            let end = Box::new(self.parse_bitshift()?);
            expr = Expr::Range {
                start: Box::new(expr),
                end,
                is_inclusive,
            };
        }

        Ok(expr)
    }
    
    fn parse_bitshift(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_term()?;
        while self.check(&TokenType::BitShiftLeft) || self.check(&TokenType::BitShiftRight) {
            let op = match self.advance() {
                TokenType::BitShiftLeft => BinaryOp::BitShiftLeft,
                TokenType::BitShiftRight => BinaryOp::BitShiftRight,
                _ => unreachable!(),
            };
            let right = Box::new(self.parse_term()?);
            expr = Expr::Binary { left: Box::new(expr), op, right };
        }
        Ok(expr)
    }

    fn parse_term(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_factor()?;

        while self.check(&TokenType::Minus) || self.check(&TokenType::Plus) {
            let op = match self.advance() {
                TokenType::Minus => BinaryOp::Subtract,
                TokenType::Plus => BinaryOp::Add,
                _ => unreachable!(),
            };
            let right = Box::new(self.parse_factor()?);
            expr = Expr::Binary {
                left: Box::new(expr),
                op,
                right,
            };
        }

        Ok(expr)
    }

    fn parse_factor(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_cast()?;

        while self.check(&TokenType::Divide) || 
              self.check(&TokenType::Multiply) || 
              self.check(&TokenType::Modulo) {
            let op = match self.advance() {
                TokenType::Divide => BinaryOp::Divide,
                TokenType::Multiply => BinaryOp::Multiply,
                TokenType::Modulo => BinaryOp::Modulo,
                _ => unreachable!(),
            };
            let right = Box::new(self.parse_cast()?);
            expr = Expr::Binary {
                left: Box::new(expr),
                op,
                right,
            };
        }

        Ok(expr)
    }
    
    fn parse_cast(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_unary()?;
        while self.match_token(&TokenType::As) {
            let target_type = self.parse_type()?;
            expr = Expr::Cast {
                expr: Box::new(expr),
                target_type,
            };
        }
        Ok(expr)
    }

    fn parse_unary(&mut self) -> Result<Expr, String> {
        if self.check(&TokenType::Not) || self.check(&TokenType::Minus) ||
           self.check(&TokenType::Increment) || self.check(&TokenType::Decrement) {
            let op = match self.advance() {
                TokenType::Not => UnaryOp::Not,
                TokenType::Minus => UnaryOp::Negate,
                TokenType::Increment => UnaryOp::PreIncrement,
                TokenType::Decrement => UnaryOp::PreDecrement,
                _ => unreachable!(),
            };
            let expr = Box::new(self.parse_unary()?);
            return Ok(Expr::Unary { op, expr });
        }
        self.parse_call()
    }

    fn parse_call(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_primary()?;

        loop {
            if self.match_token(&TokenType::LeftParen) {
                expr = self.finish_function_call(expr)?;
            } else if self.match_token(&TokenType::Dot) {
                if !self.match_token(&TokenType::Identifier(String::new())) {
                    return Err("Expected field or method name after '.'".to_string());
                }
                let name = self.get_identifier()?;
                if self.match_token(&TokenType::LeftParen) {
                    expr = self.finish_method_call(expr, name)?;
                } else {
                    expr = Expr::FieldAccess { object: Box::new(expr), field: name };
                }
            } else if self.match_token(&TokenType::LeftBracket) {
                let index_or_range = self.parse_expression()?;
                self.consume(&TokenType::RightBracket, "Expected ']' after index or slice range")?;
                
                if let Expr::Range { start, end, is_inclusive } = index_or_range {
                    // It's a slice expression
                    expr = Expr::Slice {
                        is_heap_allocated: false, // Defaulting to a reference slice
                        array: Box::new(expr),
                        start,
                        end,
                        is_inclusive,
                    };
                } else {
                    // It's a standard array index
                    expr = Expr::Index { array: Box::new(expr), index: Box::new(index_or_range) };
                }
            } else if self.match_token(&TokenType::Increment) {
                 expr = Expr::Unary { op: UnaryOp::PostIncrement, expr: Box::new(expr) };
            } else if self.match_token(&TokenType::Decrement) {
                 expr = Expr::Unary { op: UnaryOp::PostDecrement, expr: Box::new(expr) };
            } else {
                break;
            }
        }

        Ok(expr)
    }
    
    fn finish_function_call(&mut self, callee: Expr) -> Result<Expr, String> {
        let name = match callee {
            Expr::Variable(name) => name,
            _ => return Err("Invalid call target".to_string()),
        };

        let mut args = Vec::new();
        if !self.check(&TokenType::RightParen) {
            loop {
                args.push(self.parse_expression()?);
                if !self.match_token(&TokenType::Comma) {
                    break;
                }
            }
        }
        self.consume(&TokenType::RightParen, "Expected ')' after arguments")?;

        Ok(Expr::FunctionCall { name, args })
    }

    fn finish_method_call(&mut self, object: Expr, method: String) -> Result<Expr, String> {
        let mut args = Vec::new();
        if !self.check(&TokenType::RightParen) {
            loop {
                args.push(self.parse_expression()?);
                if !self.match_token(&TokenType::Comma) {
                    break;
                }
            }
        }
        self.consume(&TokenType::RightParen, "Expected ')' after method arguments")?;

        Ok(Expr::MethodCall { object: Box::new(object), method, args })
    }

    fn parse_primary(&mut self) -> Result<Expr, String> {
        match self.peek().clone() {
            TokenType::BoolLiteral(true) => { self.advance(); Ok(Expr::Literal(LiteralValue::Bool(true))) },
            TokenType::BoolLiteral(false) => { self.advance(); Ok(Expr::Literal(LiteralValue::Bool(false))) },
            TokenType::None => { self.advance(); Ok(Expr::Literal(LiteralValue::None)) },
            TokenType::IntLiteral(val) => { self.advance(); Ok(Expr::Literal(LiteralValue::Int(val))) },
            TokenType::FloatLiteral(val) => { self.advance(); Ok(Expr::Literal(LiteralValue::Float(val))) },
            TokenType::StringLiteral(val) => { self.advance(); Ok(Expr::Literal(LiteralValue::String(val))) },
            TokenType::CharLiteral(val) => { self.advance(); Ok(Expr::Literal(LiteralValue::Char(val))) },
            TokenType::Identifier(name) => {
                self.advance(); // Consume identifier
                if self.match_token(&TokenType::Scope) { // Static call: Type::method()
                    if !self.match_token(&TokenType::Identifier(String::new())) {
                        return Err("Expected method name after '::'".to_string());
                    }
                    let method = self.get_identifier()?;
                    self.consume(&TokenType::LeftParen, "Expected '(' for static call")?;
                    let mut args = Vec::new();
                    if !self.check(&TokenType::RightParen) {
                        loop {
                            args.push(self.parse_expression()?);
                            if !self.match_token(&TokenType::Comma) { break; }
                        }
                    }
                    self.consume(&TokenType::RightParen, "Expected ')' after static call arguments")?;
                    return Ok(Expr::StaticCall { type_name: name, method, args });

                } else if self.match_token(&TokenType::LeftBrace) { // Struct initialization
                    let mut fields = Vec::new();
                    if !self.check(&TokenType::RightBrace) {
                        loop {
                            if !self.match_token(&TokenType::Identifier(String::new())) {
                               return Err("Expected field name in struct initializer".to_string());
                            }
                            let field_name = self.get_identifier()?;
                            self.consume(&TokenType::Colon, "Expected ':' after field name")?;
                            let value = self.parse_expression()?;
                            fields.push((field_name, value));
                            if !self.match_token(&TokenType::Comma) { break; }
                        }
                    }
                    self.consume(&TokenType::RightBrace, "Expected '}' after struct fields")?;
                    return Ok(Expr::StructInit { name, fields });
                }
                // It was just a variable
                Ok(Expr::Variable(name))
            },
            TokenType::LeftParen => { // Grouping
                self.advance();
                let expr = self.parse_expression()?;
                self.consume(&TokenType::RightParen, "Expected ')' after expression.")?;
                Ok(expr)
            },
            TokenType::LeftBracket => { // Array initialization
                self.advance();
                let mut elements = Vec::new();
                if !self.check(&TokenType::RightBracket) {
                    loop {
                        elements.push(self.parse_expression()?);
                        if !self.match_token(&TokenType::Comma) { break; }
                    }
                }
                self.consume(&TokenType::RightBracket, "Expected ']' after array elements")?;
                Ok(Expr::ArrayInit { elements })
            },
            _ => Err(format!("Expected expression, found {:?}", self.peek())),
        }
    }
}


