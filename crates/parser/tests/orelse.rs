use lexer::Lexer;
use parser::{Expr, Item, parser::Parser, Stmt};

fn parser_for(source: &str) -> Parser {
    let mut lexer = Lexer::new(
        source.to_string(),
    );

    let tokens = lexer
        .tokenize()
        .expect("failed to tokenize test source");

    Parser::new(tokens, 0)
}

#[test]
fn parses_orelse_expression() {
    let mut parser = parser_for(
        r#"
        fn main() -> void {
            const value: i32? = None;
            const result = value orelse 10;
        }
        "#,
    );

    let items = parser
        .parse()
        .expect("orelse should parse");

    let Item::Function(function) = &items[0] else {
        panic!("expected function item");
    };

    let body = function
        .body
        .as_ref()
        .expect("function should have a body");

    let Stmt::VariableDecl {
        initializer,
        ..
    } = &body.statements[1]
    else {
        panic!("expected result variable declaration");
    };

    let Expr::OrElse {
        optional,
        fallback,
        ..
    } = initializer.as_ref()
    else {
        panic!("expected orelse expression");
    };

    assert!(matches!(
        optional.as_ref(),
        Expr::Variable { name, .. }
            if name.lexeme == "value"
    ));

    assert!(matches!(
        fallback.as_ref(),
        Expr::Literal { token, .. }
            if token.lexeme == "10"
    ));
}

#[test]
fn orelse_has_lower_precedence_than_logical_or() {
    let mut parser = parser_for(
        r#"
        fn main() -> void {
            const result = left || right orelse fallback;
        }
        "#,
    );

    let items = parser
        .parse()
        .expect("orelse precedence should parse");

    let Item::Function(function) = &items[0] else {
        panic!("expected function item");
    };

    let body = function
        .body
        .as_ref()
        .expect("function should have a body");

    let Stmt::VariableDecl {
        initializer,
        ..
    } = &body.statements[0]
    else {
        panic!("expected variable declaration");
    };

    let Expr::OrElse {
        optional,
        ..
    } = initializer.as_ref()
    else {
        panic!("expected outer orelse expression");
    };

    assert!(matches!(
        optional.as_ref(),
        Expr::Binary { .. }
    ));
}
