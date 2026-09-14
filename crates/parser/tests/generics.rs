use lexer::{Lexer, TokenType};

use parser::{
    ast::{
        Expr,
        Item,
        Stmt,
        Type,
    },
    parser::Parser,
};


fn parse(source: &str) -> Vec<Item> {
    let mut lexer =
        Lexer::new(
            source.to_string()
        );

    let tokens =
        lexer
            .tokenize()
            .expect(
                "source should lex successfully"
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
                "source should parse successfully: {errors:#?}"
            );
        })
}


fn first_call(
    items: &[Item],
) -> (
    &Expr,
    &[Type],
    &[Type],
) {
    let Item::Function(function) =
        &items[0]
    else {
        panic!(
            "expected main function"
        );
    };

    let body =
        function
            .body
            .as_ref()
            .expect(
                "main should have a body"
            );

    let Stmt::Expr(expr) =
        &body.statements[0]
    else {
        panic!(
            "expected expression statement"
        );
    };

    let Expr::FunctionCall {
        callee,
        owner_generic_args,
        generic_args,
        ..
    } = expr.as_ref()
    else {
        panic!(
            "expected function call, found {expr:#?}"
        );
    };

    (
        callee,
        owner_generic_args,
        generic_args,
    )
}


fn assert_type_name(
    ty: &Type,
    expected: &str,
) {
    let Type::Path {
        segments,
        ..
    } = ty
    else {
        panic!(
            "expected path type, found {ty:#?}"
        );
    };

    assert_eq!(
        segments.len(),
        1,
    );

    assert_eq!(
        segments[0].lexeme,
        expected,
    );
}


fn assert_path(
    ty: &Type,
    expected: &str,
) {
    let Type::Path {
        segments,
        ..
    } = ty
    else {
        panic!(
            "expected path type, found {ty:#?}"
        );
    };

    assert_eq!(
        segments.len(),
        1,
    );

    assert_eq!(
        segments[0].lexeme,
        expected,
    );
}


#[test]
fn parses_owner_generic_arguments() {
    let items =
        parse(
            r#"
fn main() -> void {
    NonNull::<i32>::dangling();
}
"#,
        );

    let (
        callee,
        owner_args,
        function_args,
    ) = first_call(&items);

    let Expr::Path {
        segments,
        ..
    } = callee
    else {
        panic!(
            "expected path callee"
        );
    };

    assert_eq!(
        segments
            .iter()
            .map(|token| {
                token.lexeme.as_str()
            })
            .collect::<Vec<_>>(),
        vec![
            "NonNull",
            "dangling",
        ],
    );

    assert_eq!(
        owner_args.len(),
        1,
    );

    assert_type_name(
        &owner_args[0],
        "i32",
    );

    assert!(
        function_args.is_empty(),
    );
}


#[test]
fn parses_function_generic_arguments() {
    let items =
        parse(
            r#"
fn main() -> void {
    NonNull::dangling::<i32>();
}
"#,
        );

    let (
        _,
        owner_args,
        function_args,
    ) = first_call(&items);

    assert!(
        owner_args.is_empty(),
    );

    assert_eq!(
        function_args.len(),
        1,
    );

    assert_type_name(
        &function_args[0],
        "i32",
    );
}


#[test]
fn parses_owner_and_function_generics_separately() {
    let items =
        parse(
            r#"
fn main() -> void {
    Foo::<i32>::convert::<u64>();
}
"#,
        );

    let (
        _,
        owner_args,
        function_args,
    ) = first_call(&items);

    assert_eq!(
        owner_args.len(),
        1,
    );

    assert_type_name(
        &owner_args[0],
        "i32",
    );

    assert_eq!(
        function_args.len(),
        1,
    );

    assert_type_name(
        &function_args[0],
        "u64",
    );
}


#[test]
fn free_function_fishtail_is_a_function_generic() {
    let items =
        parse(
            r#"
fn main() -> void {
    size_of::<Allocator>();
}
"#,
        );

    let (
        _,
        owner_args,
        function_args,
    ) = first_call(&items);

    assert!(
        owner_args.is_empty(),
    );

    assert_eq!(
        function_args.len(),
        1,
    );

    assert_type_name(
        &function_args[0],
        "Allocator",
    );
}

fn assert_nested_vec_box_i32(
    ty: &Type,
) {
    let Type::Generic {
        base,
        args,
        ..
    } = ty
    else {
        panic!(
            "expected Vec<Box<i32>>, found {ty:#?}"
        );
    };

    assert_path(
        base,
        "Vec",
    );

    assert_eq!(
        args.len(),
        1,
    );

    let Type::Generic {
        base,
        args,
        ..
    } = &args[0]
    else {
        panic!(
            "expected Box<i32>, found {:#?}",
            args[0],
        );
    };

    assert_path(
        base,
        "Box",
    );

    assert_eq!(
        args.len(),
        1,
    );

    assert_path(
        &args[0],
        "i32",
    );
}


#[test]
fn parses_nested_generic_type_without_whitespace() {
    let items =
        parse(
            r#"
fn consume(
    value: Vec<Box<i32>>
) -> void {
}
"#,
        );

    let Item::Function(
        function
    ) = &items[0]
    else {
        panic!(
            "expected function"
        );
    };

    assert_eq!(
        function.parameters.len(),
        1,
    );

    assert_nested_vec_box_i32(
        &function.parameters[0].1,
    );
}


#[test]
fn parses_three_nested_generic_closers() {
    let items =
        parse(
            r#"
fn consume(
    value: Outer<Middle<Inner<i32>>>
) -> void {
}
"#,
        );

    let Item::Function(
        function
    ) = &items[0]
    else {
        panic!(
            "expected function"
        );
    };

    let Type::Generic {
        base,
        args,
        ..
    } = &function.parameters[0].1
    else {
        panic!(
            "expected Outer<...>"
        );
    };

    assert_path(
        base,
        "Outer",
    );

    let Type::Generic {
        base,
        args,
        ..
    } = &args[0]
    else {
        panic!(
            "expected Middle<...>"
        );
    };

    assert_path(
        base,
        "Middle",
    );

    let Type::Generic {
        base,
        args,
        ..
    } = &args[0]
    else {
        panic!(
            "expected Inner<i32>"
        );
    };

    assert_path(
        base,
        "Inner",
    );

    assert_path(
        &args[0],
        "i32",
    );
}


#[test]
fn parses_nested_generic_function_argument() {
    let items =
        parse(
            r#"
fn main() -> void {
    create::<Vec<Box<i32>>>();
}
"#,
        );

    let Item::Function(
        function
    ) = &items[0]
    else {
        panic!(
            "expected main function"
        );
    };

    let body =
        function
            .body
            .as_ref()
            .expect(
                "main should have a body"
            );

    let Stmt::Expr(
        expr
    ) = &body.statements[0]
    else {
        panic!(
            "expected expression statement"
        );
    };

    let Expr::FunctionCall {
        generic_args,
        ..
    } = expr.as_ref()
    else {
        panic!(
            "expected generic function call"
        );
    };

    assert_eq!(
        generic_args.len(),
        1,
    );

    assert_nested_vec_box_i32(
        &generic_args[0],
    );
}

#[test]
fn nested_generics_can_touch_assignment_operator() {
    let items =
        parse(
            r#"
fn main() -> void {
    const value: Vec<Box<i32>>=create();
}
"#,
        );

    let Item::Function(
        function
    ) = &items[0]
    else {
        panic!(
            "expected main function"
        );
    };

    let body =
        function
            .body
            .as_ref()
            .expect(
                "main should have a body"
            );

    let Stmt::VariableDecl {
        type_annotation:
            Some(ty),
        ..
    } = &body.statements[0]
    else {
        panic!(
            "expected typed variable declaration"
        );
    };

    assert_nested_vec_box_i32(
        ty,
    );
}
