use std::{fs, sync::atomic::{AtomicUsize, Ordering}};

use analyzer::{Analyzer, Resolver};
use errors::error::HydraError;
use ir::{
    context::SemanticContext,
    hir::{HIRExprKind, HIRProgram, HIRStmt},
    types::Type,
};
use parser::module::{ModuleTree, SourceMap};

static NEXT_TEST_ID: AtomicUsize = AtomicUsize::new(0);

fn analyze(source: &str) -> Result<HIRProgram, Vec<HydraError>> {
    let id = NEXT_TEST_ID.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "hydrac-if-expression-test-{}-{id}", std::process::id()
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

fn main_body(program: &HIRProgram) -> &[HIRStmt] {
    &program.functions.iter()
        .find(|function| function.name == "main")
        .expect("main should exist")
        .body.stmts
}

#[test]
fn annotated_if_expression_has_expected_type() {
    let hir = analyze(r#"
fn main() -> void {
    const value: usize = if true {
        8;
    } else {
        4;
    };
}
"#).expect("value-producing if should analyze");

    let HIRStmt::VarDecl { init: Some(init), .. } = &main_body(&hir)[0] else {
        panic!("expected initialized variable");
    };

    assert_eq!(init.ty, Type::USIZE);

    let HIRExprKind::If { then_block, else_block: Some(else_block), .. } = &init.kind else {
        panic!("expected if expression, found {:#?}", init.kind);
    };

    let HIRStmt::Expr(then_value) = then_block.stmts.last()
        .expect("then branch should have a value") else {
        panic!("expected then branch expression");
    };

    let HIRStmt::Expr(else_value) = else_block.stmts.last()
        .expect("else branch should have a value") else {
        panic!("expected else branch expression");
    };

    assert_eq!(then_value.ty, Type::USIZE);
    assert_eq!(else_value.ty, Type::USIZE);
}

#[test]
fn unannotated_if_expression_infers_branch_type() {
    let hir = analyze(r#"
fn main() -> void {
    const value = if true {
        8;
    } else {
        4;
    };
}
"#).expect("if expression should infer its type");

    let HIRStmt::VarDecl { local, init: Some(init), .. } = &main_body(&hir)[0] else {
        panic!("expected initialized variable");
    };

    assert_eq!(local.ty, Type::I32);
    assert_eq!(init.ty, Type::I32);
}

#[test]
fn nested_else_if_produces_a_value() {
    let hir = analyze(r#"
fn main() -> void {
    const value: usize = if false {
        8;
    } else if true {
        4;
    } else {
        1;
    };
}
"#).expect("nested else-if expression should analyze");

    let HIRStmt::VarDecl { init: Some(init), .. } = &main_body(&hir)[0] else {
        panic!("expected initialized variable");
    };

    assert_eq!(init.ty, Type::USIZE);

    let HIRExprKind::If { else_block: Some(else_block), .. } = &init.kind else {
        panic!("expected outer if expression");
    };

    let HIRStmt::Expr(nested) = else_block.stmts.last()
        .expect("outer else branch should contain nested if") else {
        panic!("expected nested if expression");
    };

    assert_eq!(nested.ty, Type::USIZE);
    assert!(matches!(nested.kind, HIRExprKind::If { .. }));
}

#[test]
fn value_if_requires_else_branch() {
    let errors = analyze(r#"
fn main() -> void {
    const value: i32 = if true {
        1;
    };
}
"#).expect_err("value-producing if without else must fail");

    assert!(
        errors.iter().any(|error| {
            error.code == "S001"
                && error.message == "if expression requires an else branch"
        }),
        "expected missing-else diagnostic, got {errors:#?}",
    );
}

#[test]
fn if_expression_branches_must_match() {
    let errors = analyze(r#"
fn main() -> void {
    const value: i32 = if true {
        1;
    } else {
        false;
    };
}
"#).expect_err("incompatible if branches must fail");

    assert!(
        errors.iter().any(|error| error.code == "S001"),
        "expected branch type mismatch, got {errors:#?}",
    );
}
