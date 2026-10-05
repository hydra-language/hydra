use std::{
    fs,
    path::{
        Path,
        PathBuf,
    },
    sync::atomic::{
        AtomicUsize,
        Ordering,
    },
};

use analyzer::{
    Analyzer,
    Resolver,
};

use ir::{
    context::HIRContext,
    hir::{
        HIRExprKind,
        HIRProgram,
        HIRStmt,
    },
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
            std::env::temp_dir().join(
                format!(
                    "hydrac-analyzer-method-call-test-{}-{}",
                    std::process::id(),
                    id,
                ),
            );

        fs::create_dir_all(
            &dir,
        )
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
// ANALYZER PIPELINE
// ============================================================================

fn analyze(source: &str) -> HIRProgram {
    let project =
        TestProject::new(source);

    let mut source_map =
        SourceMap::new();

    let mut module_tree =
        ModuleTree::build(
            Path::new(
                &project.entry,
            ),
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
    ) = resolver
        .resolve()
        .unwrap_or_else(|errors| {
            panic!(
                "name resolution failed:\n{errors:#?}"
            );
        });

    Analyzer::new(
        &module_tree,
        &mut context,
        &source_map,
        name_resolver,
        global_symbols,
    )
    .analyze()
    .unwrap_or_else(|errors| {
        panic!(
            "semantic analysis failed:\n{errors:#?}"
        );
    })
}


// ============================================================================
// PROJECTED RECEIVERS
// ============================================================================

#[test]
fn method_call_on_struct_field_receiver_is_lowered() {
    let hir =
        analyze(
            r#"
            struct Inner {
                value: i32;
            }

            extension Inner {

                fn get(&self) -> i32 {
                    return self.value;
                }
            }

            struct Outer {
                inner: Inner;
            }

            extension Outer {

                fn get_inner(&self) -> i32 {
                    return self.inner::get();
                }
            }

            fn main() -> void {
            }
            "#,
        );

    let function =
        hir.functions
            .iter()
            .find(|function| {
                function.name
                    == "Outer::get_inner"
            })
            .expect(
                "Outer::get_inner should exist",
            );

    let stmt =
        function
            .body
            .stmts
            .first()
            .expect(
                "get_inner should contain a return statement",
            );

    let HIRStmt::Expr(return_expr) =
        stmt
    else {
        panic!(
            "expected expression statement, found {stmt:#?}"
        );
    };

    let HIRExprKind::Return(
        Some(value)
    ) = &return_expr.kind
    else {
        panic!(
            "expected return expression, found {:#?}",
            return_expr.kind,
        );
    };

    let HIRExprKind::Call {
        args,
        ..
    } = &value.kind
    else {
        panic!(
            "expected projected receiver to lower as a method call, found {:#?}",
            value.kind,
        );
    };

    assert_eq!(
        args.len(),
        1,
        "get() should receive only its implicit self argument",
    );

    let HIRExprKind::Borrow {
        is_mut,
        target,
    } = &args[0].kind
    else {
        panic!(
            "shared receiver should be borrowed, found {:#?}",
            args[0].kind,
        );
    };

    assert!(
        !is_mut,
        "get(&self) must receive a shared borrow",
    );

    assert!(
        matches!(
            target.kind,
            HIRExprKind::FieldAccess {
                ..
            }
        ),
        "the borrow must target the projected `self.inner` place",
    );
}
