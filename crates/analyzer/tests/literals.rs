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
    types::Type,
};

use parser::module::{
    ModuleTree,
    SourceMap,
};

static NEXT_TEST_ID: AtomicUsize =
    AtomicUsize::new(0);

struct TestProject {
    dir: PathBuf,
    entry: PathBuf,
}

impl TestProject {

    fn new(source: &str) -> Self {
        let id = NEXT_TEST_ID.fetch_add(
            1,
            Ordering::Relaxed,
        );

        let dir =
            std::env::temp_dir().join(
                format!(
                    "hydrac-analyzer-literal-test-{}-{}",
                    std::process::id(),
                    id,
                ),
            );

        fs::create_dir_all(&dir)
            .expect(
                "failed to create temporary test directory",
            );

        let entry = dir.join("main.hydra");

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

fn analyze(source: &str) -> HIRProgram {
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

fn main_body(
    program: &HIRProgram,
) -> &[HIRStmt] {
    &program
        .functions
        .iter()
        .find(|function| {
            function.name == "main"
        })
        .expect(
            "expected main function",
        )
        .body
        .stmts
}

#[test]
fn string_literals_have_const_char_slice_type() {
    let program = analyze(
        r#"
fn main() -> void {
    let text = "Aé你🦀";
}
"#,
    );

    let body =
        main_body(&program);

    let HIRStmt::VarDecl {
        init: Some(init),
        ..
    } = &body[0]
    else {
        panic!(
            "expected variable declaration"
        );
    };

    assert_eq!(
        init.ty,
        Type::CONST_REF(
            Box::new(
                Type::SLICE(
                    Box::new(Type::CHAR),
                ),
            ),
        ),
    );

    match &init.kind {
        HIRExprKind::StringLiteral(value) => {
            assert_eq!(
                value,
                "Aé你🦀",
            );

            assert_eq!(
                value.chars().count(),
                4,
            );
        }

        other => {
            panic!(
                "expected string literal, got {other:?}",
            );
        }
    }
}


#[test]
fn unicode_char_literal_has_char_type() {
    let program = analyze(
        r#"
fn main() -> void {
    let value: char = '🦀';
}
"#,
    );

    let body =
        main_body(&program);

    let HIRStmt::VarDecl {
        init: Some(init),
        ..
    } = &body[0]
    else {
        panic!(
            "expected variable declaration"
        );
    };

    assert_eq!(
        init.ty,
        Type::CHAR,
    );

    assert!(
        matches!(
            init.kind,
            HIRExprKind::CharLiteral('🦀')
        ),
        "expected Unicode char literal, got {:?}",
        init.kind,
    );
}
