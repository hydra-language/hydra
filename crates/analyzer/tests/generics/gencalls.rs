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
        HIRExpr,
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

    fn new(source: &str) -> Self {
        let id =
            NEXT_TEST_ID.fetch_add(
                1,
                Ordering::Relaxed,
            );

        let dir =
            std::env::temp_dir()
                .join(
                    format!(
                        "hydrac-generic-call-test-{}-{}",
                        std::process::id(),
                        id,
                    )
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
) -> Result<HIRProgram, Vec<HydraError>> {
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
}


fn main_init_call(
    hir: &HIRProgram,
    index: usize,
) -> &HIRExpr {
    let main =
        hir.functions
            .iter()
            .find(|function| {
                function.name == "main"
            })
            .expect(
                "main should exist"
            );

    let HIRStmt::VarDecl {
        init: Some(expr),
        ..
    } = &main.body.stmts[index]
    else {
        panic!(
            "expected initialized variable declaration"
        );
    };

    expr
}


// ============================================================================
// ASSOCIATED CALLS
// ============================================================================

#[test]
fn owner_and_function_generics_flatten_in_declaration_order() {
    let hir =
        analyze(
            r#"
struct Owner<T> {
    value: T;
}

extension<T> Owner<T> {

    fn token() -> usize {
        return 0;
    }

    fn convert<U>(
        value: U
    ) -> U {
        return value;
    }

    fn map<U>(
        &self,
        value: U
    ) -> U {
        return value;
    }
}

fn identity<T>(
    value: T
) -> T {
    return value;
}

fn use_owner(
    owner: Owner<i32>
) -> u64 {
    return owner::map::<u64>(7);
}

fn main() -> void {
    const owner =
        Owner::<i32>::token();

    const both =
        Owner::<i32>::convert::<u64>(7);

    const free =
        identity::<i32>(9);
}
"#,
        )
        .expect(
            "owner/function generic calls should analyze",
        );


    //
    // Owner::<i32>::token()
    //
    let owner_call =
        main_init_call(
            &hir,
            0,
        );

    let HIRExprKind::Call {
        generic_args,
        ..
    } = &owner_call.kind
    else {
        panic!(
            "expected associated call, found {:#?}",
            owner_call.kind,
        );
    };

    assert_eq!(
        generic_args,
        &vec![
            Type::I32,
        ],
    );

    assert_eq!(
        owner_call.ty,
        Type::USIZE,
    );


    //
    // Owner::<i32>::convert::<u64>()
    //
    let both_call =
        main_init_call(
            &hir,
            1,
        );

    let HIRExprKind::Call {
        generic_args,
        ..
    } = &both_call.kind
    else {
        panic!(
            "expected associated generic call, found {:#?}",
            both_call.kind,
        );
    };

    assert_eq!(
        generic_args,
        &vec![
            Type::I32,
            Type::U64,
        ],
    );

    assert_eq!(
        both_call.ty,
        Type::U64,
    );


    //
    // identity::<i32>()
    //
    let free_call =
        main_init_call(
            &hir,
            2,
        );

    let HIRExprKind::Call {
        generic_args,
        ..
    } = &free_call.kind
    else {
        panic!(
            "expected generic free-function call, found {:#?}",
            free_call.kind,
        );
    };

    assert_eq!(
        generic_args,
        &vec![
            Type::I32,
        ],
    );

    assert_eq!(
        free_call.ty,
        Type::I32,
    );


    //
    // owner: Owner<i32>
    // owner::map::<u64>()
    //
    let use_owner =
        hir.functions
            .iter()
            .find(|function| {
                function.name == "use_owner"
            })
            .expect(
                "use_owner should exist"
            );

    let HIRStmt::Expr(return_expr) =
        &use_owner.body.stmts[0]
    else {
        panic!(
            "expected return expression"
        );
    };

    let HIRExprKind::Return(
        Some(value)
    ) = &return_expr.kind
    else {
        panic!(
            "expected return value"
        );
    };

    let HIRExprKind::Call {
        generic_args,
        ..
    } = &value.kind
    else {
        panic!(
            "expected instance method call, found {:#?}",
            value.kind,
        );
    };

    assert_eq!(
        generic_args,
        &vec![
            Type::I32,
            Type::U64,
        ],
        "instance method calls must infer owner generics from self and append function generics",
    );

    assert_eq!(
        value.ty,
        Type::U64,
    );
}


#[test]
fn rejects_function_fishtail_for_owner_only_generic() {
    let errors =
        analyze(
            r#"
struct Owner<T> {
    value: T;
}

extension<T> Owner<T> {

    fn token() -> usize {
        return 0;
    }
}

fn main() -> void {
    const value =
        Owner::token::<i32>();
}
"#,
        )
        .expect_err(
            "token itself has no function generic parameters",
        );

    assert!(
        errors.iter().any(|error| {
            error.code == "S004"
                && error.message.contains(
                    "function expected at most 0 generic arguments, found 1"
                )
        }),
        "expected S004 for postfix generic arguments on a nongeneric function, got {errors:#?}",
    );
}
