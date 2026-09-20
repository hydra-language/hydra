use lexer::{Lexer, TokenType};
use parser::{ Expr, Item, parser::Parser, Stmt, Type };

fn parser_for(source: &str) -> Parser {
    let mut lexer = Lexer::new(source.to_string());
    let tokens = lexer.tokenize().expect("failed to tokenize test source");

    Parser::new(tokens, 0)
}

#[test]
fn parses_optional_type_suffix() {
    let mut parser = parser_for(
        r#"
        fn optional(value: i32?) -> i32? {
            return value;
        }
        "#,
    );

    let items = parser.parse().expect("optional type should parse");

    let Item::Function(function) = &items[0] else {
        panic!("expected function item");
    };

    assert!(matches!(
        &function.parameters[0].1,
        Type::Optional { inner, .. }
            if matches!(
                inner.as_ref(),
                Type::Path { segments, .. } if segments[0].lexeme == "i32"
            )
    ));

    assert!(matches!(
        function.return_type.as_ref(),
        Some(Type::Optional { inner, .. })
            if matches!(
                inner.as_ref(),
                Type::Path { segments, .. } if segments[0].lexeme == "i32"
            )
    ));
}

#[test]
fn reference_to_optional_binds_question_mark_to_pointee() {
    let mut parser = parser_for(
        r#"
        fn inspect(value: &mut i32?) -> void {
        }
        "#,
    );

    let items = parser.parse().expect("reference to optional should parse");

    let Item::Function(function) = &items[0] else {
        panic!("expected function item");
    };

    assert!(matches!(
        &function.parameters[0].1,
        Type::Borrow {
            is_mut: true,
            inner,
            ..
        } if matches!(
            inner.as_ref(),
            Type::Optional { inner, .. }
                if matches!(
                    inner.as_ref(),
                    Type::Path { segments, .. } if segments[0].lexeme == "i32"
                )
        )
    ));
}

#[test]
fn rejects_nested_optional_types_for_v1() {
    let mut parser = parser_for(
        r#"
        fn invalid(value: i32??) -> void {
        }
        "#,
    );

    let errors = parser
        .parse()
        .expect_err("nested optional types should be rejected");

    assert!(errors.iter().any(|error| {
        error.code == "P015" && error.message == "nested optional types are not currently supported"
    }));
}

#[test]
fn parses_none_as_a_literal_expression() {
    let mut parser = parser_for(
        r#"
        fn maybe() -> i32? {
            return None;
        }
        "#,
    );

    let items = parser.parse().expect("None should parse");

    let Item::Function(function) = &items[0] else {
        panic!("expected function item");
    };

    let body = function.body.as_ref().expect("function should have a body");

    let Stmt::Return {
        value: Some(value),
        ..
    } = &body.statements[0]
    else {
        panic!("expected return statement with a value");
    };

    assert!(matches!(
        value.as_ref(),
        Expr::Literal { token, .. } if token.token_type == TokenType::NONE
    ));
}
