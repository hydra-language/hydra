use lexer::{Lexer, TokenType};
use parser::parser::Parser;
use parser::{Expr, Item, MatchPattern, Stmt};

fn parse(source: &str) -> Vec<Item> {
    let mut lexer = Lexer::new(source.to_string());
    let tokens = lexer.tokenize().expect("source should lex");
    let mut parser = Parser::new(tokens, 0);

    parser.parse().expect("source should parse")
}

fn main_body(items: &[Item]) -> &[Stmt] {
    let Item::Function(function) = &items[0] else {
        panic!("expected first item to be a function");
    };

    &function
        .body
        .as_ref()
        .expect("function should have a body")
        .statements
}

#[test]
fn parses_match_arms_bindings_ranges_and_else_binding() {
    let items = parse(
        r#"
        fn main() -> void {
            match x {
                None => handle_none(),

                1: num => {
                    use_num(num);
                }

                2..=10: other => use_num(other);
            } else: remaining {
                use_remaining(remaining);
            }
        }
        "#,
    );

    let Stmt::Expr(expr) = &main_body(&items)[0] else {
        panic!("expected match expression statement");
    };

    let Expr::Match {
        value,
        arms,
        else_arm,
        ..
    } = &**expr
    else {
        panic!("expected match expression");
    };

    assert!(matches!(
        &**value,
        Expr::Variable { name, .. } if name.lexeme == "x"
    ));

    assert_eq!(arms.len(), 3);

    assert!(matches!(
        &arms[0].pattern,
        MatchPattern::Value(value)
            if matches!(
                &**value,
                Expr::Literal { token, .. }
                    if token.token_type == TokenType::NONE
            )
    ));

    assert!(arms[0].binding.is_none());

    assert!(matches!(
        &arms[1].binding,
        Some((_, name)) if name.lexeme == "num"
    ));

    let MatchPattern::Range {
        start,
        end,
        is_inclusive,
    } = &arms[2].pattern
    else {
        panic!("expected inclusive range pattern");
    };

    assert!(*is_inclusive);

    assert!(matches!(
        &**start,
        Expr::Literal { token, .. }
            if token.token_type == TokenType::IntLiteral(2)
    ));

    assert!(matches!(
        &**end,
        Expr::Literal { token, .. }
            if token.token_type == TokenType::IntLiteral(10)
    ));

    assert!(matches!(
        &arms[2].binding,
        Some((_, name)) if name.lexeme == "other"
    ));

    let else_arm = else_arm.as_ref().expect("expected else arm");

    assert!(matches!(
        &else_arm.binding,
        Some((_, name)) if name.lexeme == "remaining"
    ));
}

#[test]
fn match_is_not_optional_specific() {
    let items = parse(
        r#"
        fn main() -> void {
            match flag {
                true => on_true(),
                false => on_false(),
            }
        }
        "#,
    );

    let Stmt::Expr(expr) = &main_body(&items)[0] else {
        panic!("expected match expression statement");
    };

    let Expr::Match {
        arms,
        else_arm,
        ..
    } = &**expr
    else {
        panic!("expected match expression");
    };

    assert_eq!(arms.len(), 2);
    assert!(else_arm.is_none());
}

#[test]
fn expression_match_arms_are_normalized_into_blocks() {
    let items = parse(
        r#"
        fn main() -> void {
            match value {
                1 => one(),
                2 => two();
                3 => three(),
            }
        }
        "#,
    );

    let Stmt::Expr(expr) = &main_body(&items)[0] else {
        panic!("expected match expression statement");
    };

    let Expr::Match { arms, .. } = &**expr else {
        panic!("expected match expression");
    };

    assert_eq!(arms.len(), 3);

    assert!(
        arms.iter()
            .all(|arm| matches!(arm.body.statements.as_slice(), [Stmt::Expr(_)]))
    );
}
