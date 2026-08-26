use core::hash;

use super::token::{Token, TokenType};
use errors::error::{HydraError, Span};

pub struct Lexer {
    input: String,
    chars: Vec<char>,
    current: usize,
    start: usize,
    line: usize,
    column: usize,
    start_line: usize,
    start_column: usize,
}

impl Lexer {

    pub fn new(input: String) -> Self {
        Self {
            input: input.clone(),
            chars: input.chars().collect(),
            current: 0,
            start: 0,
            line: 1,
            column: 1,
            start_line: 1,
            start_column: 1
        }
    }

    fn error(&self, code: &'static str, message: impl Into<String>) -> HydraError {
        HydraError::new(code, message, Span {
            line: self.start_line,
            column: self.start_column,
            length: (self.current - self.start).max(1)
        })
    }

    pub fn tokenize(&mut self) -> Result<Vec<Token>, HydraError> {
        let mut tokens = Vec::new();

        while !self.is_at_end() {
            self.skip_whitespace();

            if self.is_at_end() {
                break;
            }

            self.start = self.current;
            self.start_line = self.line;
            self.start_column = self.column;

            if let Some(token_type) = self.scan_token()? {
                let lexeme = &self.input[self.start_offset()..self.current_offset()].to_string();

                tokens.push(Token {
                    token_type,
                    lexeme: lexeme.clone(),
                    span: Span {
                        line: self.start_line,
                        column: self.start_column,
                        length: self.current - self.start
                    }
                });
            }
        }

        tokens.push(Token {
            token_type: TokenType::EOF,
            lexeme: String::new(),
            span: Span {
                line: self.line,
                column: self.column,
                length: 1
            }
        });

        Ok(tokens)
    }

    fn scan_token(&mut self) -> Result<Option<TokenType>, HydraError> {
        let c = self.advance();

        let result = match c {
            '"' => return self.scan_string(),
            '\'' => return self.scan_char(),
            '(' => Some(TokenType::LeftParen),
            ')' => Some(TokenType::RightParen),
            '{' => Some(TokenType::LeftBrace),
            '}' => Some(TokenType::RightBrace),
            '[' => Some(TokenType::LeftBracket),
            ']' => Some(TokenType::RightBracket),
            ';' => Some(TokenType::Semicolon),
            '#' => Some(TokenType::Hash),
            '.' => {
                if self.peek() == '.' {
                    if self.peek_next() == '.' {
                        self.advance();
                        self.advance();

                        Some(TokenType::TripleDot)
                    } else if self.peek_next() == '=' {
                        self.advance();
                        self.advance();

                        Some(TokenType::DoubleDotEqual)
                    } else {
                        self.advance();

                        Some(TokenType::DoubleDot)
                    }
                } else {
                    Some(TokenType::Dot)
                }
            },
            ':' => if self.match_char(':') {
                Some(TokenType::DoubleColon)
            } else {
                Some(TokenType::Colon)
            },
            '&' => if self.match_char('&') {
                Some(TokenType::DoubleAmpersand)
            } else if self.match_char('=') {
                Some(TokenType::AmpersandEqual)
            } else {
                Some(TokenType::Ampersand)
            },
            '|' => if self.match_char('|') {
                Some(TokenType::DoublePipe)
            } else if self.match_char('=') {
                Some(TokenType::PipeEqual)
            } else {
                Some(TokenType::Pipe)
            },
            '^' => if self.match_char('=') {
                Some(TokenType::CarrotEqual)
            } else {
                Some(TokenType::Carrot)
            },
            '?' => Some(TokenType::QuestionMark),
            ',' => Some(TokenType::Comma),
            '+' => if self.match_char('=') {
                Some(TokenType::PlusEqual)
            } else if self.match_char('+') {
                Some(TokenType::PlusPlus)
            } else {
                Some(TokenType::Plus)
            },
            '-' => if self.match_char('=') {
                Some(TokenType::MinusEqual)
            } else if self.match_char('-') {
                Some(TokenType::MinusMinus)
            } else if self.match_char('>') {
                Some(TokenType::Arrow)
            } else {
                Some(TokenType::Minus)
            },
            '*' => if self.match_char('=') {
                Some(TokenType::StarEqual)
            } else {
                Some(TokenType::Star)
            },
            '/' => if self.match_char('/') {
                while self.peek() != '\n' && !self.is_at_end() {
                    self.advance();
                }

                None
            } else if self.match_char('*') {
                // multi line comment, consume until '*/' or EOF
                while !self.is_at_end() {
                    if self.peek() == '*' && self.peek_next() == '/' {
                        // consume '*' and '/'
                        self.advance();
                        self.advance();

                        break;
                    } else {
                        if self.peek() == '\n' {
                            self.line += 1;
                            self.column = 0;
                        }

                        self.advance();
                    }
                }

                None
            } else if self.match_char('='){
                Some(TokenType::ForwardSlashEqual)
            } else {
                Some(TokenType::ForwardSlash)
            },
            '%' => if self.match_char('=') {
                Some(TokenType::ModuloEqual)
            } else {
                Some(TokenType::Modulo)
            },
            '=' => if self.match_char('=') {
                Some(TokenType::DoubleEqual)
            } else if self.match_char('>') {
                Some(TokenType::EqualArrow)
            } else {
                Some(TokenType::Equal)
            },
            '!' => if self.match_char('=') {
                Some(TokenType::ExclamEqual)
            } else {
                Some(TokenType::ExclamationMark)
            },
            '<' => {
                if self.peek() == '<' && self.peek_next() == '=' {
                    self.advance(); // consume '<'
                    self.advance(); // consume '='

                    Some(TokenType::DoubleLeftEqual)
                } else if self.match_char('<') {
                    Some(TokenType::DoubleLeftAngle)
                } else if self.match_char('=') {
                    Some(TokenType::LessEqual)
                } else {
                    Some(TokenType::LeftAngle)
                }
            },
            '>' => {
                if self.peek() == '>' && self.peek_next() == '=' {
                    self.advance();
                    self.advance();

                    Some(TokenType::DoubleRightEqual)
                } else if self.match_char('>') {
                    Some(TokenType::DoubleRightAngle)
                } else if self.match_char('=') {
                    Some(TokenType::GreaterEqual)
                } else {
                    Some(TokenType::RightAngle)
                }
            },
            _ => {
                if c.is_ascii_digit() {
                    return self.scan_number(c);
                } else if c.is_alphabetic() || c == '_' {
                    return self.scan_identifier(c);
                } else {
                    return Err(self.error("L001", format!("unexpected character '{}'", c)));
                }
            }
        };

        Ok(result)
    }

    fn scan_number(&mut self, first_digit: char) -> Result<Option<TokenType>, HydraError> {
        // Check for hex or binary
        if first_digit == '0' {
            match self.peek() {
                'x' | 'X' => {
                    self.advance(); // consume 'x'
                    
                    while self.peek().is_ascii_hexdigit() || self.peek() == '_' {
                        self.advance();
                    }

                    let lexeme = &self.input[self.start_offset()..self.current_offset()];

                    let value = u64::from_str_radix(&lexeme[2..].replace('_', ""), 16)
                        .map_err(|_| self.error("L002", format!("invalid hexadecimal literal: '{}'", lexeme)))? as i64;

                    return Ok(Some(TokenType::IntLiteral(value)));
                }

                'b' => {
                    self.advance(); // consume 'b'
                   
                    while self.peek() == '0' || self.peek() == '1' || self.peek() == '_' {
                        self.advance();
                    }

                    let lexeme = &self.input[self.start_offset()..self.current_offset()];
                    let value = u64::from_str_radix(&lexeme[2..].replace('_', ""), 2)
                        .map_err(|_| self.error("L003", format!("invalid binary literal: '{}'", lexeme)))? as i64;

                    return Ok(Some(TokenType::IntLiteral(value)));
                }

                _ => {}
            }
        }

        // --- Decimal / Float path ---
        while self.peek().is_ascii_digit() || self.peek() == '_' {
            self.advance();
        }

        // After consuming the whole number part, check if it's a float
        if self.peek() == '.' && self.peek_next().is_ascii_digit() {
            self.advance(); // Consume the '.'

            while self.peek().is_ascii_digit() {
                self.advance();
            }

            let lexeme = &self.input[self.start_offset()..self.current_offset()];
            let value: f64 = lexeme.replace('_', "").parse()
                .map_err(|_| self.error("L004", format!("invalid float literal: '{}'", lexeme)))?;

            Ok(Some(TokenType::FloatLiteral(value)))
        } else {
            let lexeme = &self.input[self.start_offset()..self.current_offset()];

            let value = lexeme.replace('_', "").parse::<u64>()
                .map_err(|_| self.error("L005", format!("invalid integer literal: '{}'", lexeme)))? as i64;

            Ok(Some(TokenType::IntLiteral(value)))
        }
    }

    fn scan_identifier(&mut self, _first: char) -> Result<Option<TokenType>, HydraError> {
        while self.peek().is_alphanumeric() || self.peek() == '_' {
            self.advance();
        }

        let text = &self.input[self.start_offset()..self.current_offset()];

        Ok(Some(self.get_keyword_or_identifier(text)))
    }

    fn get_keyword_or_identifier(&self, text: &str) -> TokenType {
        match text {
            "isize" => TokenType::ISIZE,
            "usize" => TokenType::USIZE,
            "i8" => TokenType::I8,
            "i16" => TokenType::I16,
            "i32" => TokenType::I32,
            "i64" => TokenType::I64,
            "u8" => TokenType::U8,
            "u16" => TokenType::U16,
            "u32" => TokenType::U32,
            "u64" => TokenType::U64,
            "f32" => TokenType::F32,
            "f64" => TokenType::F64,
            "char" => TokenType::CHAR,
            "bool" => TokenType::BOOL,
            "let" => TokenType::LET,
            "const" => TokenType::CONST,
            "mut" => TokenType::MUT,
            "fn" => TokenType::FN,
            "struct" => TokenType::STRUCT,
            "extension" => TokenType::EXTENSION,
            "trait" => TokenType::TRAIT,
            "where" => TokenType::WHERE,
            "return" => TokenType::RETURN,
            "in" => TokenType::IN,
            "as" => TokenType::AS,
            "on" => TokenType::ON,
            "if" => TokenType::IF,
            "else" => TokenType::ELSE,
            "for" => TokenType::FOR,
            "foreach" => TokenType::FOREACH,
            "while" => TokenType::WHILE,
            "break" => TokenType::BREAK,
            "match" => TokenType::MATCH,
            "continue" => TokenType::CONTINUE,
            "include" => TokenType::INCLUDE,
            "mod" => TokenType::MODULE,
            "typedef" => TokenType::TYPEDEF,
            "anysize" => TokenType::ANYSIZE,
            "anytype" => TokenType::ANYTYPE,
            "extern" => TokenType::EXTERN,
            "pub" => TokenType::PUB,
            "None" => TokenType::NONE,
            "true" => TokenType::BoolLiteral(true),
            "false" => TokenType::BoolLiteral(false),
            _ => TokenType::IDENTIFIER(text.to_string()),
        }
    }

    fn advance(&mut self) -> char {
        let c = self.chars[self.current];
        self.current += 1;
        self.column += 1;

        c
    }

    fn match_char(&mut self, expected: char) -> bool {
        if self.is_at_end() || self.chars[self.current] != expected {
            false
        } else {
            self.current += 1;
            self.column += 1;
            true
        }
    }

    fn peek(&self) -> char {
        if self.is_at_end() {
            '\0'
        } else {
            self.chars[self.current]
        }
    }

    fn peek_next(&self) -> char {
        if self.current + 1 >= self.chars.len() {
            '\0'
        } else {
            self.chars[self.current + 1]
        }
    }

    fn is_at_end(&self) -> bool {
        self.current >= self.chars.len()
    }

    fn skip_whitespace(&mut self) {
        while !self.is_at_end() {
            match self.peek() {
                ' ' | '\r' | '\t' => {
                    self.advance();
                },
                '\n' => {
                    self.line += 1;
                    self.column = 0;
                    self.advance();
                },
                _ => break,
            }
        }
    }

    fn scan_string(&mut self) -> Result<Option<TokenType>, HydraError> {
        let mut value = String::new();

        while !self.is_at_end() {
            let c = self.advance();

            match c {
                '"' => {
                    return Ok(Some(TokenType::StringLiteral(value)));
                }
                '\\' => {
                    let escaped = match self.advance() {
                        'n' => '\n', // new line
                        'r' => '\r', // move cursor to beginnning of line
                        't' => '\t', // tab
                        '"' => '"',
                        '\\' => '\\',

                        other => return Err(self.error("L006", format!("invalid escape sequence: '\\{}'", other))),
                    };

                    value.push(escaped);
                }
                '\n' => {
                    self.line += 1;
                    self.column = 0;

                    value.push('\n');
                }
                _ => {
                    value.push(c);
                }
            }
        }

        Err(self.error("L007", "unterminated string literal"))
    }

    fn scan_char(&mut self) -> Result<Option<TokenType>, HydraError> {
        if self.is_at_end() {
            return Err(self.error("L008", "unterminated character literal"))
        }

        let c = match self.advance() {
            '\\' => {
                match self.advance() {
                    'n' => '\n',
                    'r' => '\r',
                    't' => '\t',
                    '\'' => '\'',
                    '\\' => '\\',

                    other => {
                        return Err(self.error("L006", format!("invalid escape sequence: '\\{}'", other)));
                    }
                }
            }

            other => other,
        };

        if self.peek() != '\'' {
            return Err(self.error("L009", "unterminated or multi-character literal"))
        }

        self.advance(); // consume closing '

        Ok(Some(TokenType::CharLiteral(c)))
    }

    fn start_offset(&self) -> usize {
        self.chars[..self.start].iter().map(|c| c.len_utf8()).sum()
    }

    fn current_offset(&self) -> usize {
        self.chars[..self.current].iter().map(|c| c.len_utf8()).sum()
    }
}
