use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};

use analyzer::{Analyzer, Resolver};

use ir::{context::HIRContext, hir::HIRProgram, types::Type};

use parser::module::{ModuleTree, SourceMap};

static NEXT_TEST_ID: AtomicUsize = AtomicUsize::new(0);

// ============================================================================
// TEST PROJECT
// ============================================================================

struct TestProject {
    dir: PathBuf,
    entry: PathBuf,
}

impl TestProject {
    fn new(source: &str) -> Self {
        let id = NEXT_TEST_ID.fetch_add(1, Ordering::Relaxed);

        let dir = std::env::temp_dir().join(format!(
            "hydrac-analyzer-generic-type-test-{}-{}",
            std::process::id(),
            id,
        ));

        fs::create_dir_all(&dir).expect("failed to create temporary test directory");

        let entry = dir.join("main.hydra");

        fs::write(&entry, source).expect("failed to write temporary Hydra source");

        Self { dir, entry }
    }
}

impl Drop for TestProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

// ============================================================================
// ANALYZER PIPELINE
// ============================================================================

fn analyze(source: &str) -> (HIRProgram, HIRContext) {
    let project = TestProject::new(source);

    let mut source_map = SourceMap::new();

    let mut module_tree = ModuleTree::build(
        Path::new(&project.entry),
        project.dir.clone(),
        &mut source_map,
    )
    .unwrap_or_else(|errors| {
        panic!("module construction failed:\n{errors:#?}");
    });

    module_tree
        .resolve_imports(&mut source_map)
        .unwrap_or_else(|errors| {
            panic!("import resolution failed:\n{errors:#?}");
        });

    module_tree
        .parse_bodies(&source_map)
        .unwrap_or_else(|errors| {
            panic!("body parsing failed:\n{errors:#?}");
        });

    let mut context = HIRContext::default();

    let resolver = Resolver::new(&module_tree, &mut context, &source_map);

    let (name_resolver, global_symbols) = resolver.resolve().unwrap_or_else(|errors| {
        panic!("name resolution failed:\n{errors:#?}");
    });

    let analyzer = Analyzer::new(
        &module_tree,
        &mut context,
        &source_map,
        name_resolver,
        global_symbols,
    );

    let hir = analyzer.analyze().unwrap_or_else(|errors| {
        panic!("semantic analysis failed:\n{errors:#?}");
    });

    (hir, context)
}

// ============================================================================
// GENERIC NOMINAL TYPES
// ============================================================================

#[test]
fn generic_instance_preserves_nominal_type_identity() {
    let (hir, context) = analyze(
        r#"
            struct Wrapper<T> {
                value: T;
            }

            fn consume(value: Wrapper<i32>) -> void {
            }

            fn main() -> void {
            }
            "#,
    );

    let wrapper_def_id = context
        .find_struct_by_name("Wrapper")
        .expect("Wrapper should have a struct definition");

    let consume = hir
        .functions
        .iter()
        .find(|function| function.name == "consume")
        .expect("consume should exist in HIR");

    let (_param_def_id, param_ty) = consume
        .params
        .first()
        .expect("consume should have one parameter");

    let Type::GENERIC_INSTANCE(base, args) = param_ty else {
        panic!("expected Wrapper<i32>, found {param_ty:#?}");
    };

    assert_eq!(
        base.def_id, wrapper_def_id,
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
