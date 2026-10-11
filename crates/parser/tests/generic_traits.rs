use lexer::Lexer;
use parser::{
    ast::{Item, Type},
    parser::Parser,
};

fn parse(source: &str) -> Vec<Item> {
    let mut lexer = Lexer::new(source.to_string());

    let tokens = lexer
        .tokenize()
        .expect("source should lex successfully");

    let mut parser = Parser::new(tokens, 1);

    parser.parse().unwrap_or_else(|errors| {
        panic!("source should parse successfully: {errors:#?}");
    })
}

#[test]
fn parses_generic_trait_parameters() {
    let items = parse(
        r#"
trait Deref<Target> {
    fn deref(&self) -> &Target;
}
"#,
    );

    let Item::Trait(decl) = &items[0] else {
        panic!("expected trait declaration");
    };

    assert_eq!(decl.name.lexeme, "Deref");
    assert_eq!(decl.generic_params.len(), 1);
    assert_eq!(decl.generic_params[0].name.lexeme, "Target");
    assert_eq!(decl.methods.len(), 1);

    let return_type = decl.methods[0]
        .return_type
        .as_ref()
        .expect("deref should have a return type");

    let Type::Borrow {
        is_mut,
        inner,
        ..
    } = return_type
    else {
        panic!("deref should return a reference");
    };

    assert!(!is_mut);

    let Type::Path { segments, .. } = inner.as_ref() else {
        panic!("deref target should be a path type");
    };

    assert_eq!(segments.len(), 1);
    assert_eq!(segments[0].lexeme, "Target");
}

#[test]
fn parses_generic_marker_trait() {
    let items = parse(
        r#"
trait Marker<T>;
"#,
    );

    let Item::Trait(decl) = &items[0] else {
        panic!("expected trait declaration");
    };

    assert_eq!(decl.name.lexeme, "Marker");
    assert_eq!(decl.generic_params.len(), 1);
    assert_eq!(decl.generic_params[0].name.lexeme, "T");
    assert!(decl.methods.is_empty());
}
