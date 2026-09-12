use lexer::{
    Lexer,
    TokenType,
};

#[test]
fn string_literal_preserves_unicode_scalars() {
    let mut lexer = Lexer::new(
        "\"Aé你🦀\"".to_string(),
    );

    let tokens = lexer
        .tokenize()
        .expect("unicode string literal should lex");

    assert_eq!(
        tokens.len(),
        2,
        "expected string literal followed by EOF",
    );

    match &tokens[0].token_type {
        TokenType::StringLiteral(value) => {
            assert_eq!(
                value,
                "Aé你🦀",
            );

            assert_eq!(
                value.chars().count(),
                4,
                "lexer must preserve Unicode scalar values",
            );
        }

        other => {
            panic!(
                "expected string literal, got {other:?}",
            );
        }
    }
}


#[test]
fn char_literal_preserves_unicode_scalar() {
    let mut lexer = Lexer::new(
        "'🦀'".to_string(),
    );

    let tokens = lexer
        .tokenize()
        .expect("unicode char literal should lex");

    assert_eq!(
        tokens.len(),
        2,
        "expected char literal followed by EOF",
    );

    assert!(
        matches!(
            tokens[0].token_type,
            TokenType::CharLiteral('🦀')
        ),
        "expected Unicode char literal, got {:?}",
        tokens[0].token_type,
    );
}
