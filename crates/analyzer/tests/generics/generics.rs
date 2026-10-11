use std::{
    fs,
    sync::atomic::{AtomicUsize, Ordering},
};

use analyzer::{Analyzer, Resolver};
use ir::{
    context::SemanticContext,
    hir::HIRProgram,
    types::Type,
};
use parser::module::{ModuleTree, SourceMap};

static NEXT_TEST_ID: AtomicUsize = AtomicUsize::new(0);

fn analyze(source: &str) -> (HIRProgram, SemanticContext) {
    let id = NEXT_TEST_ID.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "hydrac-generic-type-test-{}-{id}",
        std::process::id(),
    ));
    fs::create_dir_all(&dir).expect("failed to create test directory");

    let result = (|| {
        let entry = dir.join("main.hydra");
        fs::write(&entry, source).expect("failed to write source");

        let mut source_map = SourceMap::new();
        let mut modules = ModuleTree::build(&entry, dir.clone(), &mut source_map)
            .expect("module tree should build");
        modules.resolve_imports(&mut source_map)
            .expect("imports should resolve");
        modules.parse_bodies(&source_map)
            .expect("function bodies should parse");

        let mut context = SemanticContext::new();
        let (resolver, symbols) = Resolver::new(&modules, &mut context, &source_map)
            .resolve()
            .expect("name resolution should succeed");

        let analyzed = Analyzer::new(&modules, context, &source_map, resolver, symbols)
            .analyze()
            .expect("semantic analysis should succeed");

        (analyzed.hir, analyzed.context)
    })();

    let _ = fs::remove_dir_all(dir);
    result
}

#[test]
fn generic_instance_preserves_nominal_type_identity() {
    let (hir, context) = analyze(r#"
struct Wrapper<T> {
    value: T;
}

fn consume(value: Wrapper<i32>) -> void {}

fn main() -> void {}
"#);

    let wrapper_id = context.find_struct_by_name("Wrapper")
        .expect("Wrapper should have a struct definition");

    let consume = hir.functions.iter()
        .find(|function| function.name == "consume")
        .expect("consume should exist in HIR");

    let param_ty = &consume.params.first()
        .expect("consume should have one parameter").ty;

    let Type::GENERIC_INSTANCE(base, args) = param_ty else {
        panic!("expected Wrapper<i32>, found {param_ty:#?}");
    };

    assert_eq!(
        base.def_id, wrapper_id,
        "generic instance should preserve the nominal struct DefID",
    );
    assert_eq!(
        base.symbol, "Wrapper",
        "generic instance should preserve the printable struct symbol",
    );
    assert_eq!(
        args,
        &[Type::I32],
        "generic instance should preserve its concrete type arguments",
    );
}
