pub mod lexer;
pub mod token;

pub use lexer::Lexer;
pub use token::{Token, TokenType};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_tokens() {
        let source = "( ) { } ;";
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize().expect("Failed to tokenize");

        // Checking that the token types are as expected in order.
        let token_types: Vec<_> = tokens.iter().map(|t| &t.token_type).collect();

        let expected_types = vec![
            &TokenType::LeftParen,
            &TokenType::RightParen,
            &TokenType::LeftBrace,
            &TokenType::RightBrace,
            &TokenType::Semicolon,
            &TokenType::EOF,
        ];

        assert_eq!(token_types, expected_types);
    }

    #[test]
    fn test_string_literal() {
        let source = "\"hello\\nworld\"";
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize().expect("Failed to tokenize");

        // Expect one string literal token plus EOF
        assert_eq!(tokens.len(), 2);
        match &tokens[0].token_type {
            TokenType::StringLiteral(s) => assert_eq!(s, "hello\nworld"),
            other => panic!("Expected string literal, got {:?}", other),
        }
        assert!(matches!(tokens[1].token_type, TokenType::EOF));
    }
}
