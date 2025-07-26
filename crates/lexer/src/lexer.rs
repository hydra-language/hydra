/**
 * ========
 * lexer.rs
 * ========
 * 
 * This file is responsible for taking the source code and tokenizing it
 */

use super::token::{Token, TokenType};

pub struct Lexer<'a> {
    input: &'a str,
    chars: Vec<char>,
    current: usize,
    start: usize,
    line: usize,
    column: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(input: &'a str) -> Self {
        Self {
            input,
            chars: input.chars().collect(),
            current: 0,
            start: 0,
            line: 1,
            column: 1,
        }
    }

    pub fn tokenize(&mut self) -> Result<Vec<Token<'a>>, String> {
        let mut tokens = Vec::new();

        while !self.is_at_end() {
            self.skip_whitespace();

            if self.is_at_end() {
                break;
            }

            self.start = self.current;
            let start_line = self.line;
            let start_column = self.column;

            if let Some(token_type) = self.scan_token()? {
                let lexeme = &self.input[self.start_offset()..self.current_offset()];

                tokens.push(Token {
                    token_type,
                    lexeme,
                    line: start_line,
                    column: start_column,
                });
            }
        }

        tokens.push(Token {
            token_type: TokenType::Eof,
            lexeme: "",
            line: self.line,
            column: self.column,
        });

        Ok(tokens)
    }

    fn scan_token(&mut self) -> Result<Option<TokenType>, String> {
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
            '.' => {
                if self.peek() == '.' && self.peek_next() == '.' {
                    self.advance();
                    self.advance();

                    Some(TokenType::Ellipsis)
                } else if self.peek() == '.' {
                    self.advance();

                    Some(TokenType::RangeExclusive)
                } else if self.peek() == '.' && self.peek_next() == '=' {
                    self.advance();
                    self.advance();

                    Some(TokenType::RangeInclusive)
                }
                else {
                    Some(TokenType::Dot)
                }
            },
            ':' => if self.match_char(':') {
                Some(TokenType::DoubleColon)
            } else if self.match_char('=') {
                Some(TokenType::ArraySlice)
            } else {
                Some(TokenType::Colon)
            },
            '&' => if self.match_char('&') {
                Some(TokenType::And)
            } else {
                Some(TokenType::Reference)
            },
            '|' => if self.match_char('|') {
                Some(TokenType::Or)
            } else {
                Some(TokenType::HeapPointerBar)
            },
            '?' => Some(TokenType::Optional),
            ',' => Some(TokenType::Comma),
            '+' => if self.match_char('=') {
                Some(TokenType::PlusAssign)
            } else if self.match_char('+') {
                Some(TokenType::Increment)
            } else {
                Some(TokenType::Plus)
            },
            '-' => if self.match_char('=') {
                Some(TokenType::MinusAssign)
            } else if self.match_char('-') {
                Some(TokenType::Decrement)
            } else if self.match_char('>') {
                Some(TokenType::Arrow)
            } else {
                Some(TokenType::Minus)
            },
            '*' => if self.match_char('=') {
                Some(TokenType::MultiplyAssign)
            }
            else {
                Some(TokenType::Multiply)
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
                    }
                    else {
                        if self.peek() == '\n' {
                            self.line += 1;
                            self.column = 0;
                        }

                        self.advance();
                    }
                }
                
                None
            } else if self.match_char('='){
                Some(TokenType::DivideAssign)
            } else {
                Some(TokenType::Divide)
            },
            '%' => if self.match_char('=') {
                Some(TokenType::ModuloAssign)
            } else {
                Some(TokenType::Modulo)
            },
            '=' => if self.match_char('=') {
                Some(TokenType::Equal)
            } else {
                Some(TokenType::Assign)
            },
            '!' => if self.match_char('=') {
                Some(TokenType::NotEqual)
            } else {
                Some(TokenType::Not)
            },
            '<' => if self.match_char('=') {
                Some(TokenType::LessEqual)
            } else {
                Some(TokenType::LeftAngle)
            },
            '>' => if self.match_char('=') {
                Some(TokenType::GreaterEqual)
            } else {
                Some(TokenType::RightAngle)
            },
            '\n' => {
                self.line += 1;
                self.column = 0;

                Some(TokenType::Newline)
            }
            _ => {
                if c.is_ascii_digit() {
                    return self.scan_number(c);
                } 
                else if c.is_alphabetic() || c == '_' {
                    return self.scan_identifier(c);
                } 
                else {
                    return Err(format!("Unexpected character '{}' at line {}, column {}", c, self.line, self.column));
                }
            }
        };

        Ok(result)
    }

    fn scan_number(&mut self, _first_digit: char) -> Result<Option<TokenType>, String> {
        // Loop to consume all parts of the number (digits and underscores)
        while self.peek().is_ascii_digit() || self.peek() == '_' {
            self.advance();
        }

        // After consuming the whole number part, check if it's a float
        if self.peek() == '.' && self.peek_next().is_ascii_digit() {
            self.advance(); // Consume the '.'

            // Consume the fractional part
            while self.peek().is_ascii_digit() {
                self.advance();
            }

            // --- Float Path ---
            let lexeme = &self.input[self.start_offset()..self.current_offset()];

            // Manually remove underscores because f64::parse() does not support them
            let value: f64 = lexeme.replace('_', "").parse()
                .map_err(|_| format!("Invalid float literal: '{}'", lexeme))?;

            Ok(Some(TokenType::FloatLiteral(value)))
        } 
        else {
            // --- Integer Path ---
            let lexeme = &self.input[self.start_offset()..self.current_offset()];
            
            // .parse::<i64>() handles underscores automatically, no change needed here
            let value: i64 = lexeme.replace('_',"")
                .parse()
                .map_err(|_| format!("Invalid integer literal: '{}'", lexeme))?;
            
            Ok(Some(TokenType::IntLiteral(value)))
        }
    }

    fn scan_identifier(&mut self, _first: char) -> Result<Option<TokenType>, String> {
        while self.peek().is_alphanumeric() || self.peek() == '_' {
            self.advance();
        }

        let text = &self.input[self.start_offset()..self.current_offset()];

        Ok(Some(self.get_keyword_or_identifier(text)))
    }

    fn get_keyword_or_identifier(&self, text: &str) -> TokenType {
        match text {
            "let" => TokenType::Let,
            "const" => TokenType::Const,
            "fn" => TokenType::Function,
            "struct" => TokenType::Struct,
            "extension" => TokenType::Extension,
            "return" => TokenType::Return,
            "in" => TokenType::In,
            "as" => TokenType::As,
            "if" => TokenType::If,
            "else if" => TokenType::ElseIf,
            "else" => TokenType::Else,
            "for" => TokenType::For,
            "forEach" => TokenType::ForEach,
            "while" => TokenType::While,
            "break" => TokenType::Break,
            "skip" => TokenType::Skip,
            "include" => TokenType::Include,
            "typedef" => TokenType::Typedef,
            "None" => TokenType::None,
            "true" => TokenType::BoolLiteral(true),
            "false" => TokenType::BoolLiteral(false),
            _ => TokenType::Identifier(text.to_string()),
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
        } 
        else {
            self.current += 1;
            self.column += 1;
            true
        }
    }

    fn peek(&self) -> char {
        if self.is_at_end() {
            '\0'
        } 
        else {
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
                _ => break,
            }
        }
    }

    fn scan_string(&mut self) -> Result<Option<TokenType>, String> {
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

                        other => return Err(format!("Invalid escape sequence: '\\{}'", other)),
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

        Err(format!(
            "Unterminated string at line {}, column {}",
            self.line, self.column
        ))
    }

    fn scan_char(&mut self) -> Result<Option<TokenType>, String> {
        if self.is_at_end() {
            return Err(format!(
                "Unterminated char at line {}, column {}",
                self.line, self.column
            ));
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
                        return Err(format!(
                            "Invalid escape sequence: '\\{}' at line {}, column {}",
                            other, self.line, self.column
                        ));
                    }
                }
            }

            other => other,
        };

        if self.peek() != '\'' {
            return Err(format!(
                "Unterminated or multi-character literal at line {}, column {}",
                self.line, self.column
            ));
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
