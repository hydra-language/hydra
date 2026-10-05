use std::{
    fs,
    path::PathBuf,
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
        HIRBlock,
        HIRExpr,
        HIRExprKind,
        HIRProgram,
        HIRStmt,
    },
};

use parser::module::{
    ModuleTree,
    SourceMap,
};

static NEXT_TEST: AtomicUsize =
    AtomicUsize::new(0);

fn analyze(source: &str) -> Result<(HIRProgram, HIRContext), Vec<HydraError>> {
    let id = NEXT_TEST.fetch_add(
        1,
        Ordering::Relaxed,
    );

    let root = std::env::temp_dir().join(
        format!(
            "hydrac-print-display-{}-{id}",
            std::process::id(),
        ),
    );

    fs::create_dir_all(&root)
        .expect("test directory should be created");

    let entry = root.join(
        "main.hydra"
    );

    fs::write(
        &entry,
        source,
    )
    .expect("test source should be written");

    let stdlib = PathBuf::from(
        env!("CARGO_MANIFEST_DIR"),
    )
    .join("../../stdlib");

    let result = (|| {
        let mut source_map =
            SourceMap::new();

        let mut modules = ModuleTree::build(
            &entry,
            stdlib,
            &mut source_map,
        )?;

        modules.resolve_imports(
            &mut source_map,
        )?;

        let mut context =
            HIRContext::new();

        let (
            name_resolver,
            global_symbols,
        ) = Resolver::new(
            &modules,
            &mut context,
            &source_map,
        )
        .resolve()?;

        let program = Analyzer::new(
            &modules,
            &mut context,
            &source_map,
            name_resolver,
            global_symbols,
        )
        .analyze()?;

        Ok((
            program,
            context,
        ))
    })();

    let _ = fs::remove_dir_all(
        root
    );

    result
}

fn expr_calls_path(
    expr: &HIRExpr,
    context: &HIRContext,
    path: &str,
) -> bool {
    match &expr.kind {
        HIRExprKind::Call {
            callee,
            args,
            ..
        } => {
            let is_target = context
                .get_def(*callee)
                .is_some_and(|info| {
                    info.absolute_path.join("::")
                        == path
                });

            is_target ||
            args.iter().any(|arg| {
                expr_calls_path(
                    arg,
                    context,
                    path,
                )
            })
        }

        HIRExprKind::IntrinsicCall {
            args,
            ..
        } => {
            args.iter().any(|arg| {
                expr_calls_path(
                    arg,
                    context,
                    path,
                )
            })
        }

        HIRExprKind::Block(block) => {
            block_calls_path(
                block,
                context,
                path,
            )
        }

        HIRExprKind::Borrow {
            target,
            ..
        } |
        HIRExprKind::Dereference {
            target,
        } => {
            expr_calls_path(
                target,
                context,
                path,
            )
        }

        _ => false,
    }
}

fn block_calls_path(
    block: &HIRBlock,
    context: &HIRContext,
    path: &str,
) -> bool {
    block.stmts.iter().any(|stmt| {
        match stmt {
            HIRStmt::Expr(expr) => {
                expr_calls_path(
                    expr,
                    context,
                    path,
                )
            }

            HIRStmt::VarDecl {
                init: Some(init),
                ..
            } => {
                expr_calls_path(
                    init,
                    context,
                    path,
                )
            }

            HIRStmt::VarDecl {
                init: None,
                ..
            } => false,
        }
    })
}

fn main_function(program: &HIRProgram) -> &ir::hir::HIRFunction {
    program
        .functions
        .iter()
        .find(|function| {
            function.name == "main"
        })
        .expect("main should exist")
}

#[test]
fn print_lowers_through_formatter_write() {
    let (
        program,
        context,
    ) = analyze(
        r#"
        fn main() -> void {
            print("value = {}", 42);
        }
        "#,
    )
    .expect(
        "print with an i32 should typecheck",
    );

    let main = main_function(
        &program
    );

    assert!(
        block_calls_path(
            &main.body,
            &context,
            "core::fmt::formatter::Formatter::new",
        ),
        "print should construct a Formatter",
    );

    assert!(
        block_calls_path(
            &main.body,
            &context,
            "core::fmt::formatter::Formatter::write",
        ),
        "print should write through Formatter::write",
    );

    assert!(
        !block_calls_path(
            &main.body,
            &context,
            "core::fmt::formatter::Formatter::writeln",
        ),
        "print must not append a newline",
    );
}

#[test]
fn println_lowers_through_formatter_writeln() {
    let (
        program,
        context,
    ) = analyze(
        r#"
        fn main() -> void {
            println("value = {}", 42);
        }
        "#,
    )
    .expect(
        "println with an i32 should typecheck",
    );

    let main = main_function(
        &program
    );

    assert!(
        block_calls_path(
            &main.body,
            &context,
            "core::fmt::formatter::Formatter::new",
        ),
        "println should construct a Formatter",
    );

    assert!(
        block_calls_path(
            &main.body,
            &context,
            "core::fmt::formatter::Formatter::writeln",
        ),
        "println should terminate through Formatter::writeln",
    );
}

#[test]
fn print_requires_display() {
    let errors = match analyze(
        r#"
        struct Foo;

        fn main() -> void {
            print("{}", Foo {});
        }
        "#,
    ) {
        Ok(_) => {
            panic!(
                "formatting Foo without Display should fail"
            );
        }

        Err(errors) => errors,
    };

    assert!(
        errors.iter().any(|error| {
            error.message.contains("Foo") &&
            error.message.contains("Display")
        }),
        "expected missing Display diagnostic, found {errors:?}",
    );
}

#[test]
fn println_requires_display() {
    let errors = match analyze(
        r#"
        struct Foo;

        fn main() -> void {
            println("{}", Foo {});
        }
        "#,
    ) {
        Ok(_) => {
            panic!(
                "formatting Foo without Display should fail"
            );
        }

        Err(errors) => errors,
    };

    assert!(
        errors.iter().any(|error| {
            error.message.contains("Foo") &&
            error.message.contains("Display")
        }),
        "expected missing Display diagnostic, found {errors:?}",
    );
}
