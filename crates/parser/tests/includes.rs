use lexer::Lexer;

use parser::{
    ast::{
        Item,
        Type,
    },
    parser::Parser,
};


// ============================================================================
// HELPERS
// ============================================================================

fn parse(
    source: &str,
) -> Vec<Item> {
    let mut lexer =
        Lexer::new(
            source.to_string()
        );

    let tokens =
        lexer
            .tokenize()
            .expect(
                "source should lex",
            );

    let mut parser =
        Parser::new(
            tokens,
            1,
        );

    parser
        .parse()
        .unwrap_or_else(|errors| {
            panic!(
                "source should parse: {errors:#?}"
            );
        })
}


// ============================================================================
// PRIMITIVE INCLUDE NAMES
// ============================================================================

#[test]
fn selective_include_accepts_primitive_type_names() {
    let items =
        parse(
            r#"
include core::primitives::{
    isize,
    i8,
    i16,
    i32,
    i64,

    usize,
    u8,
    u16,
    u32,
    u64,

    f32,
    f64,

    bool,
    char,

    slice,
};
"#,
        );

    let Item::Include(
        include
    ) = &items[0]
    else {
        panic!(
            "expected include item"
        );
    };

    let Type::Path {
        segments,
        ..
    } = &include.path
    else {
        panic!(
            "expected include path"
        );
    };

    let path:
        Vec<_> =
        segments
            .iter()
            .map(|token| {
                token.lexeme.as_str()
            })
            .collect();

    assert_eq!(
        path,
        vec![
            "core",
            "primitives",
        ],
    );

    let symbols =
        include
            .symbols
            .as_ref()
            .expect(
                "expected selective symbols",
            );

    let names:
        Vec<_> =
        symbols
            .iter()
            .map(|token| {
                token.lexeme.as_str()
            })
            .collect();

    assert_eq!(
        names,
        vec![
            "isize",
            "i8",
            "i16",
            "i32",
            "i64",

            "usize",
            "u8",
            "u16",
            "u32",
            "u64",

            "f32",
            "f64",

            "bool",
            "char",

            "slice",
        ],
    );
}


#[test]
fn direct_include_accepts_primitive_type_name() {
    let items =
        parse(
            r#"
include core::primitives::u64;
"#,
        );

    let Item::Include(
        include
    ) = &items[0]
    else {
        panic!(
            "expected include item"
        );
    };

    let Type::Path {
        segments,
        ..
    } = &include.path
    else {
        panic!(
            "expected include path"
        );
    };

    let path:
        Vec<_> =
        segments
            .iter()
            .map(|token| {
                token.lexeme.as_str()
            })
            .collect();

    assert_eq!(
        path,
        vec![
            "core",
            "primitives",
            "u64",
        ],
    );
}
