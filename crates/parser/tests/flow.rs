use lexer::{Lexer, TokenType};
use parser::{
    ast::{Expr, Item, Stmt},
    parser::Parser,
};

fn parse(source: &str) -> Vec<Item> {
    let mut lexer = Lexer::new(source.to_string());

    let tokens = lexer
        .tokenize()
        .expect("source should lex successfully");

    let mut parser = Parser::new(tokens, 1);

    parser
        .parse()
        .unwrap_or_else(|errors| {
            panic!("source should parse successfully: {errors:#?}");
        })
}

fn main_body(items: &[Item]) -> &[Stmt] {
    let Item::Function(function) = &items[0] else {
        panic!("expected first item to be a function");
    };

    assert_eq!(function.name.lexeme, "main");

    &function
        .body
        .as_ref()
        .expect("main should have a body")
        .statements
}

fn int_literal(expr: &Expr) -> i64 {
    let Expr::Literal { token, .. } = expr else {
        panic!("expected integer literal, found {expr:#?}");
    };

    let TokenType::IntLiteral(value) = token.token_type else {
        panic!("expected integer literal token, found {token:#?}");
    };

    value
}

#[test]
fn parses_empty_function_body() {
    let items = parse(
        r#"
        fn main() -> void {
        }
        "#,
    );

    assert_eq!(items.len(), 1);
    assert!(main_body(&items).is_empty());
}

#[test]
fn parses_empty_while_body() {
    let items = parse(
        r#"
        fn main() -> void {
            while true {
            }
        }
        "#,
    );

    let body = main_body(&items);

    assert_eq!(body.len(), 1);

    let Stmt::Expr(expr) = &body[0] else {
        panic!("expected while expression");
    };

    let Expr::While {
        condition,
        body,
        ..
    } = expr.as_ref()
    else {
        panic!("expected while expression, found {expr:#?}");
    };

    let Expr::Literal { token, .. } = condition.as_ref() else {
        panic!("expected boolean condition");
    };

    assert!(matches!(
        token.token_type,
        TokenType::BoolLiteral(true)
    ));

    assert!(body.statements.is_empty());
}

#[test]
fn parses_for_range() {
    let items = parse(
        r#"
        fn main() -> void {
            for i in 0..5 {
                println("hello");
            }
        }
        "#,
    );

    let body = main_body(&items);

    assert_eq!(body.len(), 1);

    let Stmt::Expr(expr) = &body[0] else {
        panic!("expected for expression");
    };

    let Expr::For {
        variable,
        start,
        end,
        is_inclusive,
        body,
        ..
    } = expr.as_ref()
    else {
        panic!("expected for expression, found {expr:#?}");
    };

    assert_eq!(variable.lexeme, "i");
    assert_eq!(int_literal(start), 0);
    assert_eq!(int_literal(end), 5);
    assert!(!is_inclusive);
    assert_eq!(body.statements.len(), 1);
}

#[test]
fn parses_reverse_for_range_without_changing_bounds() {
    let items = parse(
        r#"
        fn main() -> void {
            for i in 5..0 {
                println("{}", i);
            }
        }
        "#,
    );

    let body = main_body(&items);

    let Stmt::Expr(expr) = &body[0] else {
        panic!("expected for expression");
    };

    let Expr::For {
        variable,
        start,
        end,
        is_inclusive,
        ..
    } = expr.as_ref()
    else {
        panic!("expected for expression, found {expr:#?}");
    };

    assert_eq!(variable.lexeme, "i");
    assert_eq!(int_literal(start), 5);
    assert_eq!(int_literal(end), 0);
    assert!(!is_inclusive);
}

#[test]
fn parses_inclusive_for_range() {
    let items = parse(
        r#"
        fn main() -> void {
            for i in 0..=5 {
                println("{}", i);
            }
        }
        "#,
    );

    let body = main_body(&items);

    let Stmt::Expr(expr) = &body[0] else {
        panic!("expected for expression");
    };

    let Expr::For {
        start,
        end,
        is_inclusive,
        ..
    } = expr.as_ref()
    else {
        panic!("expected for expression, found {expr:#?}");
    };

    assert_eq!(int_literal(start), 0);
    assert_eq!(int_literal(end), 5);
    assert!(*is_inclusive);
}

#[test]
fn parses_nested_empty_blocks() {
    let items = parse(
        r#"
        fn main() -> void {
            while true {
                if true {
                }
            }
        }
        "#,
    );

    let body = main_body(&items);

    let Stmt::Expr(expr) = &body[0] else {
        panic!("expected while expression");
    };

    let Expr::While { body, .. } = expr.as_ref() else {
        panic!("expected while expression");
    };

    assert_eq!(body.statements.len(), 1);

    let Stmt::Expr(expr) = &body.statements[0] else {
        panic!("expected if expression");
    };

    let Expr::If {
        then_branch,
        else_branch,
        ..
    } = expr.as_ref()
    else {
        panic!("expected if expression");
    };

    assert!(then_branch.statements.is_empty());
    assert!(else_branch.is_none());
}

#[test]
fn parses_break_and_continue_inside_loop() {
    let items = parse(
        r#"
        fn main() -> void {
            while true {
                continue;
                break;
            }
        }
        "#,
    );

    let body = main_body(&items);

    let Stmt::Expr(expr) = &body[0] else {
        panic!("expected while expression");
    };

    let Expr::While { body, .. } = expr.as_ref() else {
        panic!("expected while expression");
    };

    assert_eq!(body.statements.len(), 2);

    assert!(matches!(
        body.statements[0],
        Stmt::Continue { .. }
    ));

    assert!(matches!(
        body.statements[1],
        Stmt::Break { .. }
    ));
}

#[test]
fn parses_conditional_break_and_continue() {
    let items = parse(
        r#"
        fn main() -> void {
            while true {
                break if true;
                continue if false;
            }
        }
        "#,
    );

    let body = main_body(&items);

    let Stmt::Expr(expr) = &body[0] else {
        panic!("expected while expression");
    };

    let Expr::While { body, .. } = expr.as_ref() else {
        panic!("expected while expression");
    };

    assert_eq!(body.statements.len(), 2);

    let Stmt::Break {
        condition: Some(condition),
        ..
    } = &body.statements[0]
    else {
        panic!("expected conditional break");
    };

    let Expr::Literal { token, .. } = condition.as_ref() else {
        panic!("expected boolean break condition");
    };

    assert!(matches!(
        token.token_type,
        TokenType::BoolLiteral(true)
    ));

    let Stmt::Continue {
        condition: Some(condition),
        ..
    } = &body.statements[1]
    else {
        panic!("expected conditional continue");
    };

    let Expr::Literal { token, .. } = condition.as_ref() else {
        panic!("expected boolean continue condition");
    };

    assert!(matches!(
        token.token_type,
        TokenType::BoolLiteral(false)
    ));
}
