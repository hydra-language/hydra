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
        "hydrac-orelse-test-{}-{id}", std::process::id()
    ));
    fs::create_dir_all(&root).expect("failed to create test directory");

    let result = (|| {
        let input = root.join("main.hydra");
        fs::write(&input, source).expect("failed to write Hydra source");

        let mut source_map = SourceMap::new();
        let mut modules = ModuleTree::build(&input, root.clone(), &mut source_map)?;
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

#[test]
fn orelse_eliminates_optional_type() {
    let hir = analyze(r#"
fn main() -> void {
    const value: i32? = None;
    const result: i32 = value orelse 10;
}
"#).expect("orelse should eliminate optionality");

    let main = hir.functions.iter()
        .find(|function| function.name == "main")
        .expect("main should be lowered");

    let HIRStmt::VarDecl { local, init: Some(init), .. } = &main.body.stmts[1] else {
        panic!("expected initialized result declaration");
    };

    assert_eq!(local.ty, Type::I32);
    assert_eq!(init.ty, Type::I32);

    let HIRExprKind::OrElse { optional, fallback } = &init.kind else {
        panic!("expected HIR orelse expression");
    };

    assert_eq!(optional.ty, Type::OPTIONAL(Box::new(Type::I32)));
    assert_eq!(fallback.ty, Type::I32);
}

#[test]
fn orelse_rejects_non_optional_left_operand() {
    let errors = analyze(r#"
fn main() -> void {
    const value = 10;
    const result = value orelse 20;
}
"#).expect_err("orelse should require an optional left operand");

    assert!(
        errors.iter().any(|error| {
            error.code == "S001"
                && error.message.contains("`orelse` requires an optional left operand")
        }),
        "expected non-optional operand diagnostic, got {errors:#?}",
    );
}

#[test]
fn basic_orelse_rejects_optional_fallback() {
    let errors = analyze(r#"
fn main() -> void {
    const value: i32? = None;
    const fallback: i32? = None;
    const result = value orelse fallback;
}
"#).expect_err("basic orelse fallback should produce plain T");

    assert!(
        errors.iter().any(|error| error.code == "S001"),
        "expected optional fallback type error, got {errors:#?}",
    );
}
