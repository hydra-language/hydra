use std::{fs, sync::atomic::{AtomicUsize, Ordering}};

use analyzer::{Analyzer, Resolver};
use errors::error::HydraError;
use ir::{
    context::SemanticContext,
    hir::{HIRExprKind, HIRFunction, HIRProgram, HIRStmt},
    types::Type,
};
use parser::module::{ModuleTree, SourceMap};

static NEXT_TEST_ID: AtomicUsize = AtomicUsize::new(0);

fn analyze(source: &str) -> Result<HIRProgram, Vec<HydraError>> {
    let id = NEXT_TEST_ID.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "hydrac-optionals-test-{}-{id}", std::process::id()
    ));
    fs::create_dir_all(&root).expect("failed to create test directory");

    let result = (|| {
        let entry = root.join("main.hydra");
        fs::write(&entry, source).expect("failed to write Hydra source");

        let mut source_map = SourceMap::new();
        let mut modules = ModuleTree::build(&entry, root.clone(), &mut source_map)?;
        modules.resolve_imports(&mut source_map)?;
        modules.parse_bodies(&source_map)?;

        let mut context = SemanticContext::new();
        let (resolver, symbols) =
            Resolver::new(&modules, &mut context, &source_map).resolve()?;

        Analyzer::new(&modules, context, &source_map, resolver, symbols)
            .analyze()
            .map(|analyzed| analyzed.hir)
    })();

    let _ = fs::remove_dir_all(root);
    result
}

fn function<'a>(hir: &'a HIRProgram, name: &str) -> &'a HIRFunction {
    hir.functions.iter()
        .find(|function| function.name == name)
        .unwrap_or_else(|| panic!("expected function `{name}`"))
}

fn optional_i32() -> Type {
    Type::OPTIONAL(Box::new(Type::I32))
}

#[test]
fn lowers_optional_types_into_hir() {
    let hir = analyze(r#"
fn identity_optional(value: i32?) -> i32? {
    return value;
}

fn inspect(value: &mut i32?) -> void {}

fn main() -> void {}
"#).expect("semantic analysis should succeed");

    let identity = function(&hir, "identity_optional");
    assert_eq!(identity.params[0].ty, optional_i32());
    assert_eq!(identity.return_type, optional_i32());

    let inspect = function(&hir, "inspect");
    assert_eq!(inspect.params[0].ty, Type::REF(Box::new(optional_i32())));
}

#[test]
fn none_uses_annotated_optional_type() {
    let hir = analyze(r#"
fn main() -> void {
    const value: i32? = None;
}
"#).expect("typed None should be accepted");

    let HIRStmt::VarDecl { local, init: Some(init), .. } = &function(&hir, "main").body.stmts[0] else {
        panic!("expected initialized variable declaration");
    };

    assert_eq!(local.ty, optional_i32());
    assert_eq!(init.ty, optional_i32());
    assert!(matches!(init.kind, HIRExprKind::NONE));
}

#[test]
fn none_uses_function_return_type_as_context() {
    let hir = analyze(r#"
fn maybe() -> i32? {
    return None;
}

fn main() -> void {}
"#).expect("return None should use the function return type");

    let HIRStmt::Expr(return_expr) = &function(&hir, "maybe").body.stmts[0] else {
        panic!("expected return expression");
    };
    let HIRExprKind::Return(Some(value)) = &return_expr.kind else {
        panic!("expected value-returning return expression");
    };

    assert_eq!(value.ty, optional_i32());
    assert!(matches!(value.kind, HIRExprKind::NONE));
}

#[test]
fn none_without_type_context_is_rejected() {
    let errors = analyze(r#"
fn main() -> void {
    const value = None;
}
"#).expect_err("untyped None should be rejected");

    assert!(
        errors.iter().any(|error| {
            error.code == "S007" && error.message == "cannot infer the type of `None`"
        }),
        "expected None inference diagnostic, got {errors:#?}",
    );
}

#[test]
fn none_in_non_optional_context_is_rejected() {
    let errors = analyze(r#"
fn main() -> void {
    const value: i32 = None;
}
"#).expect_err("None should require an optional expected type");

    assert!(
        errors.iter().any(|error| {
            error.code == "S001"
                && error.message == "`None` requires an optional type, found `i32`"
        }),
        "expected invalid None context diagnostic, got {errors:#?}",
    );
}

#[test]
fn plain_value_is_promoted_into_optional_variable() {
    let hir = analyze(r#"
fn main() -> void {
    const value: i32? = 42;
}
"#).expect("plain value should promote into optional type");

    let HIRStmt::VarDecl { local, init: Some(init), .. } = &function(&hir, "main").body.stmts[0] else {
        panic!("expected initialized variable declaration");
    };

    assert_eq!(local.ty, optional_i32());
    assert_eq!(init.ty, optional_i32());

    let HIRExprKind::OptionalPresent(value) = &init.kind else {
        panic!("expected present optional construction");
    };

    assert_eq!(value.ty, Type::I32);
    assert!(matches!(value.kind, HIRExprKind::IntLiteral(42)));
}

#[test]
fn plain_value_is_promoted_into_optional_return() {
    let hir = analyze(r#"
fn maybe() -> i32? {
    return 42;
}

fn main() -> void {}
"#).expect("plain return value should promote into optional return type");

    let HIRStmt::Expr(return_expr) = &function(&hir, "maybe").body.stmts[0] else {
        panic!("expected return expression");
    };
    let HIRExprKind::Return(Some(value)) = &return_expr.kind else {
        panic!("expected value-returning return expression");
    };

    assert_eq!(value.ty, optional_i32());

    let HIRExprKind::OptionalPresent(inner) = &value.kind else {
        panic!("expected present optional construction");
    };

    assert_eq!(inner.ty, Type::I32);
    assert!(matches!(inner.kind, HIRExprKind::IntLiteral(42)));
}

#[test]
fn optional_value_is_not_implicitly_demoted() {
    let errors = analyze(r#"
fn maybe() -> i32? {
    return None;
}

fn main() -> void {
    const optional: i32? = maybe();
    const value: i32 = optional;
}
"#).expect_err("optional values must not silently become plain values");

    assert!(
        errors.iter().any(|error| {
            error.code == "S001"
                && error.message.contains("expected i32")
                && error.message.contains("found i32?")
        }),
        "expected optional-to-plain mismatch, got {errors:#?}",
    );
}

#[test]
fn value_if_can_mix_present_value_and_none() {
    let hir = analyze(r#"
fn maybe(condition: bool) -> i32? {
    return if condition {
        42;
    } else {
        None;
    };
}

fn main() -> void {}
"#).expect("value-producing if should preserve optionality across its branches");

    let HIRStmt::Expr(return_expr) = &function(&hir, "maybe").body.stmts[0] else {
        panic!("expected return expression");
    };
    let HIRExprKind::Return(Some(value)) = &return_expr.kind else {
        panic!("expected value-returning return expression");
    };

    assert_eq!(value.ty, optional_i32());
}
