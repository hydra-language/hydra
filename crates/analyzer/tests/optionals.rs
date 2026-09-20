use std::{fs, time::{SystemTime, UNIX_EPOCH}};

use analyzer::{Analyzer, Resolver};
use ir::{context::HIRContext, hir::{HIRExprKind, HIRProgram, HIRStmt}, types::Type};
use parser::module::{ModuleTree, SourceMap};

fn analyze(source: &str) -> Result<HIRProgram, Vec<errors::error::HydraError>> {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_nanos();

    let root = std::env::temp_dir().join(format!(
        "hydrac_optional_test_{}_{}",
        std::process::id(),
        unique,
    ));

    fs::create_dir_all(&root).expect("failed to create temporary test directory");

    let input = root.join("main.hydra");
    fs::write(&input, source).expect("failed to write temporary Hydra source");

    let mut source_map = SourceMap::new();

    let result = (|| {
        let mut program = ModuleTree::build(
            &input,
            root.clone(),
            &mut source_map,
        )?;

        program.resolve_imports(&mut source_map)?;

        let mut context = HIRContext::default();
        let resolver = Resolver::new(
            &program,
            &mut context,
            &source_map,
        );

        let (name_resolver, global_symbols) = resolver.resolve()?;

        Analyzer::new(
            &program,
            &mut context,
            &source_map,
            name_resolver,
            global_symbols,
        )
        .analyze()
    })();

    fs::remove_dir_all(root).ok();

    result
}

#[test]
fn lowers_optional_types_into_hir() {
    let hir = analyze(
        r#"
        fn identity_optional(value: i32?) -> i32? {
            return value;
        }

        fn inspect(value: &mut i32?) -> void {
        }

        fn main() -> void {
        }
        "#,
    )
    .expect("semantic analysis should succeed");

    let identity = hir
        .functions
        .iter()
        .find(|function| function.name == "identity_optional")
        .expect("identity_optional should be lowered");

    assert_eq!(
        identity.params[0].1,
        Type::OPTIONAL(Box::new(Type::I32)),
    );

    assert_eq!(
        identity.return_type,
        Type::OPTIONAL(Box::new(Type::I32)),
    );

    let inspect = hir
        .functions
        .iter()
        .find(|function| function.name == "inspect")
        .expect("inspect should be lowered");

    assert_eq!(
        inspect.params[0].1,
        Type::REF(Box::new(Type::OPTIONAL(Box::new(Type::I32)))),
    );
}

#[test]
fn none_uses_annotated_optional_type() {
    let hir = analyze(
        r#"
        fn main() -> void {
            const value: i32? = None;
        }
        "#,
    )
    .expect("typed None should be accepted");

    let main = hir
        .functions
        .iter()
        .find(|function| function.name == "main")
        .expect("main should be lowered");

    let HIRStmt::VarDecl {
        ty,
        init: Some(init),
        ..
    } = &main.body.stmts[0]
    else {
        panic!("expected initialized variable declaration");
    };

    assert_eq!(
        *ty,
        Type::OPTIONAL(Box::new(Type::I32)),
    );

    assert_eq!(
        init.ty,
        Type::OPTIONAL(Box::new(Type::I32)),
    );

    assert!(matches!(init.kind, HIRExprKind::NONE));
}

#[test]
fn none_uses_function_return_type_as_context() {
    let hir = analyze(
        r#"
        fn maybe() -> i32? {
            return None;
        }

        fn main() -> void {
        }
        "#,
    )
    .expect("return None should use the function return type");

    let maybe = hir
        .functions
        .iter()
        .find(|function| function.name == "maybe")
        .expect("maybe should be lowered");

    let HIRStmt::Expr(return_expr) = &maybe.body.stmts[0] else {
        panic!("expected return expression");
    };

    let HIRExprKind::Return(Some(value)) = &return_expr.kind else {
        panic!("expected value-returning return expression");
    };

    assert_eq!(
        value.ty,
        Type::OPTIONAL(Box::new(Type::I32)),
    );

    assert!(matches!(value.kind, HIRExprKind::NONE));
}

#[test]
fn none_without_type_context_is_rejected() {
    let errors = analyze(
        r#"
        fn main() -> void {
            const value = None;
        }
        "#,
    )
    .expect_err("untyped None should be rejected");

    assert!(errors.iter().any(|error| {
        error.code == "S007" && error.message == "cannot infer the type of `None`"
    }));
}

#[test]
fn none_in_non_optional_context_is_rejected() {
    let errors = analyze(
        r#"
        fn main() -> void {
            const value: i32 = None;
        }
        "#,
    )
    .expect_err("None should require an optional expected type");

    assert!(errors.iter().any(|error| {
        error.code == "S001" && error.message == "`None` requires an optional type, found `i32`"
    }));
}

#[test]
fn plain_value_is_promoted_into_optional_variable() {
    let hir = analyze(
        r#"
        fn main() -> void {
            const value: i32? = 42;
        }
        "#,
    )
    .expect("plain value should promote into optional type");

    let main = hir
        .functions
        .iter()
        .find(|function| function.name == "main")
        .expect("main should be lowered");

    let HIRStmt::VarDecl {
        ty,
        init: Some(init),
        ..
    } = &main.body.stmts[0]
    else {
        panic!("expected initialized variable declaration");
    };

    assert_eq!(
        *ty,
        Type::OPTIONAL(Box::new(Type::I32)),
    );

    assert_eq!(
        init.ty,
        Type::OPTIONAL(Box::new(Type::I32)),
    );

    let HIRExprKind::OptionalPresent(value) = &init.kind else {
        panic!("expected present optional construction");
    };

    assert_eq!(value.ty, Type::I32);
    assert!(matches!(value.kind, HIRExprKind::IntLiteral(42)));
}

#[test]
fn plain_value_is_promoted_into_optional_return() {
    let hir = analyze(
        r#"
        fn maybe() -> i32? {
            return 42;
        }

        fn main() -> void {
        }
        "#,
    )
    .expect("plain return value should promote into optional return type");

    let maybe = hir
        .functions
        .iter()
        .find(|function| function.name == "maybe")
        .expect("maybe should be lowered");

    let HIRStmt::Expr(return_expr) = &maybe.body.stmts[0] else {
        panic!("expected return expression");
    };

    let HIRExprKind::Return(Some(value)) = &return_expr.kind else {
        panic!("expected value-returning return expression");
    };

    assert_eq!(
        value.ty,
        Type::OPTIONAL(Box::new(Type::I32)),
    );

    let HIRExprKind::OptionalPresent(inner) = &value.kind else {
        panic!("expected present optional construction");
    };

    assert_eq!(inner.ty, Type::I32);
    assert!(matches!(inner.kind, HIRExprKind::IntLiteral(42)));
}

#[test]
fn optional_value_is_not_implicitly_demoted() {
    let errors = analyze(
        r#"
        fn maybe() -> i32? {
            return None;
        }

        fn main() -> void {
            const optional: i32? = maybe();
            const value: i32 = optional;
        }
        "#,
    )
    .expect_err("optional values must not silently become plain values");

    assert!(errors.iter().any(|error| {
        error.code == "S001"
            && error.message.contains("expected i32")
            && error.message.contains("found i32?")
    }));
}

#[test]
fn value_if_can_mix_present_value_and_none() {
    let hir = analyze(
        r#"
        fn maybe(condition: bool) -> i32? {
            return if condition {
                42;
            } else {
                None;
            };
        }

        fn main() -> void {
        }
        "#,
    )
    .expect("value-producing if should preserve optionality across its branches");

    let maybe = hir
        .functions
        .iter()
        .find(|function| function.name == "maybe")
        .expect("maybe should be lowered");

    let HIRStmt::Expr(return_expr) = &maybe.body.stmts[0] else {
        panic!("expected return expression");
    };

    let HIRExprKind::Return(Some(value)) = &return_expr.kind else {
        panic!("expected value-returning return expression");
    };

    assert_eq!(
        value.ty,
        Type::OPTIONAL(Box::new(Type::I32)),
    );
}
