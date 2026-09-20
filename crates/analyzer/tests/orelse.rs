use std::{
    fs,
    time::{SystemTime, UNIX_EPOCH},
};

use analyzer::{Analyzer, Resolver};
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
use parser::module::{ModuleTree, SourceMap};

fn analyze(
    source: &str,
) -> Result<HIRProgram, Vec<HydraError>> {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_nanos();

    let root = std::env::temp_dir().join(
        format!(
            "hydrac_orelse_test_{}_{}",
            std::process::id(),
            unique,
        ),
    );

    fs::create_dir_all(&root)
        .expect("failed to create temporary test directory");

    let input = root.join("main.hydra");

    fs::write(
        &input,
        source,
    )
    .expect("failed to write temporary Hydra source");

    let mut source_map = SourceMap::new();

    let result = (|| {
        let mut program = ModuleTree::build(
            &input,
            root.clone(),
            &mut source_map,
        )?;

        program.resolve_imports(
            &mut source_map,
        )?;

        let mut context = HIRContext::default();

        let resolver = Resolver::new(
            &program,
            &mut context,
            &source_map,
        );

        let (
            name_resolver,
            global_symbols,
        ) = resolver.resolve()?;

        Analyzer::new(
            &program,
            &mut context,
            &source_map,
            name_resolver,
            global_symbols,
        )
        .analyze()
    })();

    fs::remove_dir_all(root).ok();

    result
}

#[test]
fn orelse_eliminates_optional_type() {
    let hir = analyze(
        r#"
        fn main() -> void {
            const value: i32? = None;
            const result: i32 = value orelse 10;
        }
        "#,
    )
    .expect("orelse should eliminate optionality");

    let main = hir
        .functions
        .iter()
        .find(|function| function.name == "main")
        .expect("main should be lowered");

    let HIRStmt::VarDecl {
        ty,
        init: Some(init),
        ..
    } = &main.body.stmts[1]
    else {
        panic!("expected initialized result declaration");
    };

    assert_eq!(
        *ty,
        Type::I32,
    );

    assert_eq!(
        init.ty,
        Type::I32,
    );

    let HIRExprKind::OrElse {
        optional,
        fallback,
    } = &init.kind
    else {
        panic!("expected HIR orelse expression");
    };

    assert_eq!(
        optional.ty,
        Type::OPTIONAL(Box::new(Type::I32)),
    );

    assert_eq!(
        fallback.ty,
        Type::I32,
    );
}

#[test]
fn orelse_rejects_non_optional_left_operand() {
    let errors = analyze(
        r#"
        fn main() -> void {
            const value = 10;
            const result = value orelse 20;
        }
        "#,
    )
    .expect_err(
        "orelse should require an optional left operand",
    );

    assert!(errors.iter().any(|error| {
        error.code == "S001"
            && error
                .message
                .contains(
                    "`orelse` requires an optional left operand",
                )
    }));
}

#[test]
fn basic_orelse_rejects_optional_fallback() {
    let errors = analyze(
        r#"
        fn main() -> void {
            const value: i32? = None;
            const fallback: i32? = None;
            const result = value orelse fallback;
        }
        "#,
    )
    .expect_err(
        "basic orelse fallback should produce plain T",
    );

    assert!(errors.iter().any(|error| {
        error.code == "S001"
    }));
}
