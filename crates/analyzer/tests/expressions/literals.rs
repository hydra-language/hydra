use std::{fs, sync::atomic::{AtomicUsize, Ordering}};

use analyzer::{Analyzer, Resolver};
use ir::{
    context::SemanticContext,
    hir::{HIRExprKind, HIRProgram, HIRStmt},
    types::Type,
};
use parser::module::{ModuleTree, SourceMap};

static NEXT_TEST_ID: AtomicUsize = AtomicUsize::new(0);

fn analyze(source: &str) -> HIRProgram {
    let id = NEXT_TEST_ID.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "hydrac-analyzer-literals-{}-{id}", std::process::id()
    ));
    fs::create_dir_all(&root).expect("failed to create test directory");

    let result = (|| {
        let entry = root.join("main.hydra");
        fs::write(&entry, source).expect("failed to write Hydra source");

        let mut source_map = SourceMap::new();
        let mut modules = ModuleTree::build(&entry, root.clone(), &mut source_map)
            .expect("module tree should build");

        modules.resolve_imports(&mut source_map)
            .expect("imports should resolve");
        modules.parse_bodies(&source_map)
            .expect("function bodies should parse");

        let mut context = SemanticContext::new();
        let (resolver, symbols) = Resolver::new(&modules, &mut context, &source_map)
            .resolve()
            .expect("name resolution should succeed");

        Analyzer::new(&modules, context, &source_map, resolver, symbols)
            .analyze()
            .expect("semantic analysis should succeed")
            .hir
    })();

    let _ = fs::remove_dir_all(root);
    result
}

fn main_body(program: &HIRProgram) -> &[HIRStmt] {
    &program.functions.iter()
        .find(|function| function.name == "main")
        .expect("expected main function")
        .body.stmts
}

#[test]
fn string_literals_have_const_u8_slice_type() {
    let program = analyze(r#"
fn main() -> void {
    let text = "Aé你🦀";
}
"#);

    let HIRStmt::VarDecl { init: Some(init), .. } = &main_body(&program)[0] else {
        panic!("expected initialized variable declaration");
    };

    assert_eq!(
        init.ty,
        Type::CONST_REF(Box::new(Type::SLICE(Box::new(Type::U8)))),
    );

    let HIRExprKind::StringLiteral(value) = &init.kind else {
        panic!("expected string literal, got {:?}", init.kind);
    };

    assert_eq!(value, "Aé你🦀");
    assert_eq!(value.len(), 10, "string literal length must use UTF-8 bytes");
}

#[test]
fn indexing_string_literal_produces_u8() {
    let program = analyze(r#"
fn main() -> void {
    let text = "abc";
    let byte = text[0];
}
"#);

    let HIRStmt::VarDecl { init: Some(init), .. } = &main_body(&program)[1] else {
        panic!("expected initialized byte variable declaration");
    };

    assert_eq!(init.ty, Type::U8, "indexing &[u8] string data must produce u8");
}

#[test]
fn unicode_char_literal_has_char_type() {
    let program = analyze(r#"
fn main() -> void {
    let value: char = '🦀';
}
"#);

    let HIRStmt::VarDecl { init: Some(init), .. } = &main_body(&program)[0] else {
        panic!("expected initialized variable declaration");
    };

    assert_eq!(init.ty, Type::CHAR);
    assert!(
        matches!(init.kind, HIRExprKind::CharLiteral('🦀')),
        "expected Unicode char literal, got {:?}",
        init.kind,
    );
}
