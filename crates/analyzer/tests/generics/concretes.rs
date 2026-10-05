use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};

use analyzer::{
    monomorphizer::Monomorphizer,
    Analyzer,
    Resolver,
};

use ir::{
    context::{DefKind, HIRContext},
    hir::HIRProgram,
    types::Type,
};

use parser::module::{ModuleTree, SourceMap};

static NEXT_TEST_ID: AtomicUsize = AtomicUsize::new(0);

struct TestProject {
    dir: PathBuf,
    entry: PathBuf,
}

impl TestProject {

    fn new(source: &str) -> Self {
        let id = NEXT_TEST_ID.fetch_add(1, Ordering::Relaxed);

        let dir = std::env::temp_dir().join(format!(
            "hydrac-concrete-struct-field-test-{}-{}",
            std::process::id(),
            id,
        ));

        fs::create_dir_all(&dir)
            .expect("failed to create temporary test directory");

        let entry = dir.join("main.hydra");

        fs::write(&entry, source)
            .expect("failed to write temporary Hydra source");

        Self { dir, entry }
    }
}

impl Drop for TestProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn monomorphize(source: &str) -> (HIRContext, HIRProgram) {
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

    let resolver = Resolver::new(
        &module_tree,
        &mut context,
        &source_map,
    );

    let (name_resolver, global_symbols) = resolver
        .resolve()
        .unwrap_or_else(|errors| {
            panic!("name resolution failed:\n{errors:#?}");
        });

    let hir = Analyzer::new(
        &module_tree,
        &mut context,
        &source_map,
        name_resolver,
        global_symbols,
    )
        .analyze()
        .unwrap_or_else(|errors| {
            panic!("semantic analysis failed:\n{errors:#?}");
        });

    let hir = Monomorphizer::new(
        &mut context,
        hir,
    )
        .run();

    (context, hir)
}

#[test]
fn concrete_generic_field_of_nongeneric_struct_is_materialized() {
    let (context, _) = monomorphize(
        r#"
struct Buffer<T> {
    value: T;
}

struct Text {
    buffer: Buffer<u8>;
}

fn consume(text: Text) -> void {
}

fn main() -> void {
}
"#,
    );

    let text_def_id = context
        .find_struct_by_name("Text")
        .expect("Text should exist");

    let text_info = context
        .get_def(text_def_id)
        .expect("Text definition should exist");

    let DefKind::Struct { fields, .. } = &text_info.kind else {
        panic!("Text should be a struct");
    };

    let (_, field_ty, _) = &fields[0];

    let Type::STRUCT(buffer_ref) = field_ty else {
        panic!(
        "expected Buffer<u8> field to be materialized, found {field_ty:#?}"
    );
    };

    assert!(
        buffer_ref.symbol.ends_with("Buffer__u8"),
        "expected Buffer<u8> specialization, got `{}`",
        buffer_ref.symbol,
    );

    let buffer_info = context
        .get_def(buffer_ref.def_id)
        .expect("specialized Buffer<u8> definition should exist");

    let DefKind::Struct {
        fields,
        generic_params,
    } = &buffer_info.kind
    else {
        panic!("Buffer<u8> specialization should be a struct");
    };

    assert!(
        generic_params.is_empty(),
        "specialized Buffer<u8> must not retain generic parameters",
    );

    assert_eq!(
        fields[0].1,
        Type::U8,
        "specialized Buffer<u8> field should be u8",
    );
}
