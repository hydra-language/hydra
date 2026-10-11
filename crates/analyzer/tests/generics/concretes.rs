use std::{
    fs,
    sync::atomic::{AtomicUsize, Ordering},
};

use analyzer::{Analyzer, Resolver, monomorphizer::Monomorphizer};
use ir::{
    context::SemanticContext,
    hir::MonomorphizedProgram,
    types::Type,
};
use parser::module::{ModuleTree, SourceMap};

static NEXT_TEST_ID: AtomicUsize = AtomicUsize::new(0);

fn monomorphize(source: &str) -> MonomorphizedProgram {
    let id = NEXT_TEST_ID.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "hydrac-concrete-field-test-{}-{id}",
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
            .expect("bodies should parse");

        let mut context = SemanticContext::new();
        let (resolver, symbols) = Resolver::new(&modules, &mut context, &source_map)
            .resolve()
            .expect("name resolution should succeed");

        let analyzed = Analyzer::new(&modules, context, &source_map, resolver, symbols)
            .analyze()
            .expect("semantic analysis should succeed");

        Monomorphizer::new(analyzed).run()
    })();

    let _ = fs::remove_dir_all(dir);
    result
}

#[test]
fn concrete_generic_field_of_nongeneric_struct_is_materialized() {
    let program = monomorphize(r#"
struct Buffer<T> {
    value: T;
}

struct Text {
    buffer: Buffer<u8>;
}

fn consume(text: Text) -> void {}

fn main() -> void {}
"#);

    let text = program.type_info.structs.values()
        .find(|info| info.type_ref.symbol == "Text")
        .expect("Text should have a concrete struct definition");

    let Type::STRUCT(buffer_ref) = &text.fields[0] else {
        panic!(
            "expected Buffer<u8> to be materialized, found {:?}",
            text.fields[0],
        );
    };

    assert!(
        buffer_ref.symbol.ends_with("Buffer__u8"),
        "expected Buffer<u8> specialization, got `{}`",
        buffer_ref.symbol,
    );

    let buffer = program.type_info.structs.get(&buffer_ref.def_id)
        .expect("specialized Buffer<u8> definition should exist");

    assert_eq!(
        buffer.fields,
        vec![Type::U8],
        "specialized Buffer<u8> must contain a concrete u8 field",
    );

    assert_eq!(
        buffer.type_ref.def_id,
        buffer_ref.def_id,
        "specialized field must reference the registered concrete struct",
    );
}
