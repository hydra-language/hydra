use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};

use analyzer::{
    Analyzer,
    Resolver,
    monomorphizer::Monomorphizer,
};

use ir::{
    context::HIRContext,
    hir::HIRProgram,
};

use parser::module::{
    ModuleTree,
    SourceMap,
};


static NEXT_TEST_ID: AtomicUsize =
    AtomicUsize::new(0);


// ============================================================================
// TEST PROJECT
// ============================================================================

struct TestProject {
    dir: PathBuf,
    entry: PathBuf,
}

impl TestProject {
    fn new(source: &str) -> Self {
        let id =
            NEXT_TEST_ID.fetch_add(
                1,
                Ordering::Relaxed,
            );

        let dir =
            std::env::temp_dir().join(format!(
                "hydrac-analyzer-monomorphization-test-{}-{}",
                std::process::id(),
                id,
            ));

        fs::create_dir_all(&dir)
            .expect(
                "failed to create temporary test directory",
            );

        let entry =
            dir.join("main.hydra");

        fs::write(
            &entry,
            source,
        )
        .expect(
            "failed to write temporary Hydra source",
        );

        Self {
            dir,
            entry,
        }
    }
}

impl Drop for TestProject {
    fn drop(&mut self) {
        let _ =
            fs::remove_dir_all(
                &self.dir,
            );
    }
}


// ============================================================================
// COMPILER PIPELINE
// ============================================================================

fn monomorphize(
    source: &str,
) -> HIRProgram {
    let project =
        TestProject::new(source);

    let mut source_map =
        SourceMap::new();

    let mut module_tree =
        ModuleTree::build(
            Path::new(&project.entry),
            project.dir.clone(),
            &mut source_map,
        )
        .unwrap_or_else(|errors| {
            panic!(
                "module construction failed:\n{errors:#?}"
            );
        });

    module_tree
        .resolve_imports(
            &mut source_map,
        )
        .unwrap_or_else(|errors| {
            panic!(
                "import resolution failed:\n{errors:#?}"
            );
        });

    module_tree
        .parse_bodies(
            &source_map,
        )
        .unwrap_or_else(|errors| {
            panic!(
                "body parsing failed:\n{errors:#?}"
            );
        });

    let mut context =
        HIRContext::default();

    let resolver =
        Resolver::new(
            &module_tree,
            &mut context,
            &source_map,
        );

    let (
        name_resolver,
        global_symbols,
    ) =
        resolver
            .resolve()
            .unwrap_or_else(|errors| {
                panic!(
                    "name resolution failed:\n{errors:#?}"
                );
            });

    let analyzer =
        Analyzer::new(
            &module_tree,
            &mut context,
            &source_map,
            name_resolver,
            global_symbols,
        );

    let hir =
        analyzer
            .analyze()
            .unwrap_or_else(|errors| {
                panic!(
                    "semantic analysis failed:\n{errors:#?}"
                );
            });

    Monomorphizer::new(
        &mut context,
        hir,
    )
    .run()
}


// ============================================================================
// FUNCTION INSTANCES
// ============================================================================

#[test]
fn reuses_existing_function_instance_for_same_type_arguments() {
    let hir =
        monomorphize(
            r#"
            fn identity<T>(value: T) -> T {
                return value;
            }

            fn main() -> void {
                let first = identity(1);
                let second = identity(2);
            }
            "#,
        );

    let specializations =
        hir.functions
            .iter()
            .filter(|function| {
                function.name
                    == "identity__i32"
            })
            .collect::<Vec<_>>();

    assert_eq!(
        specializations.len(),
        1,
        "identical function instances should reuse the same specialization",
    );
}


#[test]
fn creates_distinct_function_instances_for_different_type_arguments() {
    let hir =
        monomorphize(
            r#"
            fn identity<T>(value: T) -> T {
                return value;
            }

            fn main() -> void {
                let integer =
                    identity(1);

                let boolean =
                    identity(true);
            }
            "#,
        );

    let i32_instances =
        hir.functions
            .iter()
            .filter(|function| {
                function.name
                    == "identity__i32"
            })
            .count();

    let bool_instances =
        hir.functions
            .iter()
            .filter(|function| {
                function.name
                    == "identity__bool"
            })
            .count();

    assert_eq!(
        i32_instances,
        1,
        "identity<i32> should produce exactly one specialization",
    );

    assert_eq!(
        bool_instances,
        1,
        "identity<bool> should produce a distinct specialization",
    );
}
