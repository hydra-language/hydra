use lexer::Lexer;

use parser::ast::{
    Expr,
    Item,
    Stmt,
};
use parser::parser::Parser;

fn parse(source: &str) -> Vec<Item> {
    let mut lexer = Lexer::new(
        source.to_string(),
    );

    let tokens = lexer
        .tokenize()
        .expect("source should tokenize");

    Parser::new(
        tokens,
        1,
    )
    .parse()
    .expect("source should parse")
}

fn first_initializer(items: &[Item]) -> &Expr {
    let Item::Function(function) = &items[0] else {
        panic!("expected function");
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

    initializer
}

#[test]
fn dereference_binds_tighter_than_cast() {
    let items = parse(
        r#"
        fn test(ptr: *const i8) -> void {
            const value = *ptr as i64;
        }
        "#,
    );

    let Expr::Cast {
        value,
        ..
    } = first_initializer(&items)
    else {
        panic!("expected cast to be the outer expression");
    };

    assert!(
        matches!(
            value.as_ref(),
            Expr::Dereference { .. }
        ),
        "expected `*ptr as i64` to parse as `(*ptr) as i64`",
    );
}

#[test]
fn unary_negation_binds_tighter_than_cast() {
    let items = parse(
        r#"
        fn test(value: i32) -> void {
            const result = -value as i64;
        }
        "#,
    );

    let Expr::Cast {
        value,
        ..
    } = first_initializer(&items)
    else {
        panic!("expected cast to be the outer expression");
    };

    assert!(
        matches!(
            value.as_ref(),
            Expr::Unary { .. }
        ),
        "expected `-value as i64` to parse as `(-value) as i64`",
    );
}

#[test]
fn call_binds_tighter_than_cast() {
    let items = parse(
        r#"
        fn value() -> i32 {
            return 1;
        }

        fn test() -> void {
            const result = value() as i64;
        }
        "#,
    );

    let Item::Function(function) = &items[1] else {
        panic!("expected test function");
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

    let Expr::Cast {
        value,
        ..
    } = initializer.as_ref()
    else {
        panic!("expected cast to be the outer expression");
    };

    assert!(
        matches!(
            value.as_ref(),
            Expr::FunctionCall { .. }
        ),
        "expected `value() as i64` to cast the result of the call",
    );
}
