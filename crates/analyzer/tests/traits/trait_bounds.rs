use std::{
    fs,
    sync::atomic::{AtomicUsize, Ordering},
};

use analyzer::{Analyzer, Resolver};
use errors::error::HydraError;
use ir::{context::SemanticContext, hir::HIRProgram};
use parser::module::{ModuleTree, SourceMap};

static NEXT_TEST_DIR: AtomicUsize = AtomicUsize::new(0);

fn analyze(source: &str) -> Result<HIRProgram, Vec<HydraError>> {
    let id = NEXT_TEST_DIR.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "hydrac-trait-bounds-{}-{id}",
        std::process::id(),
    ));
    let entry = root.join("main.hydra");
    let stdlib = root.join("stdlib");

    fs::create_dir_all(&stdlib).expect("test directory should be created");
    fs::write(&entry, source).expect("test source should be written");

    let result = (|| {
        let mut source_map = SourceMap::new();
        let mut program = ModuleTree::build(&entry, stdlib, &mut source_map)?;
        program.resolve_imports(&mut source_map)?;
        program.parse_bodies(&source_map)?;

        let mut context = SemanticContext::new();
        let (name_resolver, global_symbols) =
            Resolver::new(&program, &mut context, &source_map).resolve()?;

        let analyzed = Analyzer::new(
            &program,
            context,
            &source_map,
            name_resolver,
            global_symbols,
        ).analyze()?;

        Ok(analyzed.hir)
    })();

    let _ = fs::remove_dir_all(root);
    result
}

#[test]
fn generic_bound_allows_trait_method_call_and_concrete_instantiation() {
    let result = analyze(r#"
        trait Printable {
            fn print(&self) -> void;
        }

        extension Printable on i32 {
            fn print(&self) -> void {}
        }

        fn show<T>(value: T) -> void
        where T: Printable
        {
            value::print();
        }

        fn main() -> void {
            show(42);
        }
    "#);

    assert!(
        result.is_ok(),
        "bounded generic call should be accepted: {result:?}",
    );
}

#[test]
fn generic_trait_method_call_without_bound_is_rejected() {
    let errors = analyze(r#"
        trait Printable {
            fn print(&self) -> void;
        }

        extension Printable on i32 {
            fn print(&self) -> void {}
        }

        fn show<T>(value: T) -> void {
            value::print();
        }

        fn main() -> void {
            show(42);
        }
    "#).expect_err("generic method use without a bound should fail");

    assert!(
        errors.iter().any(|error| error.code == "S005"),
        "expected method-resolution failure, found {errors:?}",
    );
}

#[test]
fn concrete_instantiation_must_satisfy_trait_bound() {
    let errors = analyze(r#"
        trait Printable {
            fn print(&self) -> void;
        }

        struct Foo;

        fn show<T>(value: T) -> void
        where T: Printable
        {}

        fn main() -> void {
            show(Foo {});
        }
    "#).expect_err("Foo does not implement Printable");

    assert!(
        errors.iter().any(|error| {
            error.code == "S028"
                && error.message.contains("Foo")
                && error.message.contains("Printable")
        }),
        "expected unsatisfied trait-bound diagnostic, found {errors:?}",
    );
}

#[test]
fn extension_bound_is_available_inside_method_body() {
    let result = analyze(r#"
        trait Printable {
            fn print(&self) -> void;
        }

        extension Printable on i32 {
            fn print(&self) -> void {}
        }

        struct Wrapper<T> {
            value: T;
        }

        extension<T> Wrapper<T>
        where T: Printable
        {
            fn show(&self) -> void {
                self.value::print();
            }
        }

        fn main() -> void {
            const wrapper: Wrapper<i32> = Wrapper {
                .value = 42,
            };

            wrapper::show();
        }
    "#);

    assert!(
        result.is_ok(),
        "extension bound should be available in method body: {result:?}",
    );
}
