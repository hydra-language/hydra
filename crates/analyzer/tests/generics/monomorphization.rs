use std::{
    fs,
    sync::atomic::{AtomicUsize, Ordering},
};

use analyzer::{Analyzer, Resolver, monomorphizer::Monomorphizer};
use ir::{
    context::SemanticContext,
    hir::{HIRStmt, MonomorphizedProgram},
    types::Type,
};
use parser::module::{ModuleTree, SourceMap};

static NEXT_TEST_ID: AtomicUsize = AtomicUsize::new(0);

fn monomorphize(source: &str) -> MonomorphizedProgram {
    let id = NEXT_TEST_ID.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "hydrac-monomorphization-test-{}-{id}",
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
fn reuses_existing_function_instance_for_same_type_arguments() {
    let program = monomorphize(r#"
fn identity<T>(value: T) -> T {
    return value;
}

fn main() -> void {
    let first = identity(1);
    let second = identity(2);
}
"#);

    let count = program.functions.iter()
        .filter(|function| function.name == "identity__i32")
        .count();

    assert_eq!(
        count, 1,
        "identical function instances should reuse the same specialization",
    );
}

#[test]
fn creates_distinct_function_instances_for_different_type_arguments() {
    let program = monomorphize(r#"
fn identity<T>(value: T) -> T {
    return value;
}

fn main() -> void {
    let integer = identity(1);
    let boolean = identity(true);
}
"#);

    let i32_instances = program.functions.iter()
        .filter(|function| function.name == "identity__i32")
        .count();

    let bool_instances = program.functions.iter()
        .filter(|function| function.name == "identity__bool")
        .count();

    assert_eq!(
        i32_instances, 1,
        "identity<i32> should produce exactly one specialization",
    );
    assert_eq!(
        bool_instances, 1,
        "identity<bool> should produce a distinct specialization",
    );
}

#[test]
fn generic_specializations_keep_independent_local_types() {
    let program = monomorphize(r#"
fn foo<T>(value: T) -> T {
    let local = value;
    return local;
}

fn main() -> void {
    let integer = foo(10);
    let boolean = foo(true);
}
"#);

    let i32_function = program.functions.iter()
        .find(|function| function.name == "foo__i32")
        .expect("foo<i32> specialization should exist");

    let bool_function = program.functions.iter()
        .find(|function| function.name == "foo__bool")
        .expect("foo<bool> specialization should exist");

    let HIRStmt::VarDecl { local: i32_local, .. } = &i32_function.body.stmts[0] else {
        panic!("foo<i32> should begin with a variable declaration");
    };

    let HIRStmt::VarDecl { local: bool_local, .. } = &bool_function.body.stmts[0] else {
        panic!("foo<bool> should begin with a variable declaration");
    };

    assert_eq!(
        i32_local.ty,
        Type::I32,
        "foo<i32> should specialize its local variable to i32",
    );
    assert_eq!(
        bool_local.ty,
        Type::BOOL,
        "foo<bool> should specialize its local variable to bool",
    );

    assert_eq!(i32_function.return_type, Type::I32);
    assert_eq!(bool_function.return_type, Type::BOOL);
}
