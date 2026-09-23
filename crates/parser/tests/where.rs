use lexer::Lexer;
use parser::parser::Parser;
use parser::{Item, Type};

fn parse(source: &str) -> Vec<Item> {
    let mut lexer = Lexer::new(source.to_string());
    let tokens = lexer.tokenize().expect("source should lex");

    Parser::new(tokens, 1)
        .parse()
        .expect("source should parse")
}

fn path_name(ty: &Type) -> &str {
    let Type::Path { segments, .. } = ty else {
        panic!("expected path type, found {ty:?}");
    };

    segments.last().unwrap().lexeme.as_str()
}

#[test]
fn parses_multiple_trait_bounds_and_predicates() {
    let items = parse(
        r#"
        trait Display;
        trait Debug;
        trait Clone;

        fn show<T, U>(value: T, other: U) -> void
        where T: Display, Debug, U: Clone
        {}
        "#,
    );

    let function = items
        .iter()
        .find_map(|item| {
            let Item::Function(function) = item else {
                return None;
            };

            (function.name.lexeme == "show").then_some(function)
        })
        .expect("show function should be present");

    let where_clause = function
        .where_clause
        .as_ref()
        .expect("where clause should be present");

    assert_eq!(where_clause.predicates.len(), 2);

    assert_eq!(
        path_name(&where_clause.predicates[0].target_type),
        "T",
    );

    assert_eq!(
        where_clause.predicates[0]
            .bound_traits
            .iter()
            .map(path_name)
            .collect::<Vec<_>>(),
        vec!["Display", "Debug"],
    );

    assert_eq!(
        path_name(&where_clause.predicates[1].target_type),
        "U",
    );

    assert_eq!(
        where_clause.predicates[1]
            .bound_traits
            .iter()
            .map(path_name)
            .collect::<Vec<_>>(),
        vec!["Clone"],
    );
}
