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

use errors::error::HydraError;

use ir::{
    context::HIRContext,
    hir::{
        HIRExprKind,
        HIRProgram,
        HIRStmt,
    },
    types::Type,
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

    fn new(
        source: &str,
    ) -> Self {
        let id =
            NEXT_TEST_ID.fetch_add(
                1,
                Ordering::Relaxed,
            );

        let dir =
            std::env::temp_dir().join(
                format!(
                    "hydrac-if-expression-test-{}-{}",
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
            dir.join(
                "main.hydra"
            );

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

fn analyze(
    source: &str,
) -> Result<
    HIRProgram,
    Vec<HydraError>,
> {
    let project =
        TestProject::new(
            source
        );

    let mut source_map =
        SourceMap::new();

    let mut module_tree =
        ModuleTree::build(
            Path::new(
                &project.entry
            ),
            project.dir.clone(),
            &mut source_map,
        )
        .unwrap_or_else(
            |errors| {
                panic!(
                    "module construction failed:\n{errors:#?}"
                );
            },
        );

    module_tree
        .resolve_imports(
            &mut source_map,
        )
        .unwrap_or_else(
            |errors| {
                panic!(
                    "import resolution failed:\n{errors:#?}"
                );
            },
        );

    module_tree
        .parse_bodies(
            &source_map,
        )
        .unwrap_or_else(
            |errors| {
                panic!(
                    "body parsing failed:\n{errors:#?}"
                );
            },
        );

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
            .unwrap_or_else(
                |errors| {
                    panic!(
                        "name resolution failed:\n{errors:#?}"
                    );
                },
            );

    Analyzer::new(
        &module_tree,
        &mut context,
        &source_map,
        name_resolver,
        global_symbols,
    )
    .analyze()
}


fn main_body(
    program: &HIRProgram,
) -> &[HIRStmt] {
    &program
        .functions
        .iter()
        .find(
            |function| {
                function.name
                    == "main"
            },
        )
        .expect(
            "main should exist",
        )
        .body
        .stmts
}


// ============================================================================
// IF EXPRESSIONS
// ============================================================================

#[test]
fn annotated_if_expression_has_expected_type() {
    let hir =
        analyze(
            r#"
fn main() -> void {
    const value: usize = if true {
        8;
    } else {
        4;
    };
}
"#,
        )
        .expect(
            "value-producing if should analyze",
        );

    let HIRStmt::VarDecl {
        init: Some(init),
        ..
    } = &main_body(
        &hir
    )[0]
    else {
        panic!(
            "expected initialized variable"
        );
    };

    assert_eq!(
        init.ty,
        Type::USIZE,
    );

    let HIRExprKind::If {
        then_block,
        else_block:
            Some(
                else_block
            ),
        ..
    } = &init.kind
    else {
        panic!(
            "expected if expression, found {:#?}",
            init.kind,
        );
    };

    let HIRStmt::Expr(
        then_value
    ) =
        then_block
            .stmts
            .last()
            .expect(
                "then branch should have value",
            )
    else {
        panic!(
            "expected then branch expression"
        );
    };

    let HIRStmt::Expr(
        else_value
    ) =
        else_block
            .stmts
            .last()
            .expect(
                "else branch should have value",
            )
    else {
        panic!(
            "expected else branch expression"
        );
    };

    assert_eq!(
        then_value.ty,
        Type::USIZE,
    );

    assert_eq!(
        else_value.ty,
        Type::USIZE,
    );
}


#[test]
fn unannotated_if_expression_infers_branch_type() {
    let hir =
        analyze(
            r#"
fn main() -> void {
    const value = if true {
        8;
    } else {
        4;
    };
}
"#,
        )
        .expect(
            "if expression should infer its type",
        );

    let HIRStmt::VarDecl {
        ty,
        init: Some(init),
        ..
    } = &main_body(
        &hir
    )[0]
    else {
        panic!(
            "expected initialized variable"
        );
    };

    assert_eq!(
        *ty,
        Type::I32,
    );

    assert_eq!(
        init.ty,
        Type::I32,
    );
}


#[test]
fn nested_else_if_produces_a_value() {
    let hir =
        analyze(
            r#"
fn main() -> void {
    const value: usize = if false {
        8;
    } else if true {
        4;
    } else {
        1;
    };
}
"#,
        )
        .expect(
            "nested else-if expression should analyze",
        );

    let HIRStmt::VarDecl {
        init: Some(init),
        ..
    } = &main_body(
        &hir
    )[0]
    else {
        panic!(
            "expected initialized variable"
        );
    };

    assert_eq!(
        init.ty,
        Type::USIZE,
    );

    let HIRExprKind::If {
        else_block:
            Some(
                else_block
            ),
        ..
    } = &init.kind
    else {
        panic!(
            "expected outer if expression"
        );
    };

    let HIRStmt::Expr(
        nested
    ) =
        else_block
            .stmts
            .last()
            .expect(
                "outer else branch should contain nested if",
            )
    else {
        panic!(
            "expected nested if expression"
        );
    };

    assert_eq!(
        nested.ty,
        Type::USIZE,
    );

    assert!(
        matches!(
            nested.kind,
            HIRExprKind::If {
                ..
            }
        ),
    );
}


#[test]
fn value_if_requires_else_branch() {
    let errors =
        analyze(
            r#"
fn main() -> void {
    const value: i32 = if true {
        1;
    };
}
"#,
        )
        .expect_err(
            "value-producing if without else must fail",
        );

    assert!(
        errors.iter().any(
            |error| {
                error.code == "S001"
                    && error.message
                        == "if expression requires an else branch"
            }
        ),
        "expected missing-else error, got {errors:#?}",
    );
}


#[test]
fn if_expression_branches_must_match() {
    let errors =
        analyze(
            r#"
fn main() -> void {
    const value: i32 = if true {
        1;
    } else {
        false;
    };
}
"#,
        )
        .expect_err(
            "incompatible if branches must fail",
        );

    assert!(
        errors.iter().any(
            |error| {
                error.code
                    == "S001"
            }
        ),
        "expected branch type error, got {errors:#?}",
    );
}
