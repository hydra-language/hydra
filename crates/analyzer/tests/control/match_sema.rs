use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

use analyzer::{Analyzer, Resolver};
use ir::{
    context::{DefID, DefKind, HIRContext},
    hir::{HIRFunction, HIRProgram, HIRStmt},
    types::Type,
};
use parser::{
    module::{ModuleTree, SourceMap},
    Expr,
    Item,
    NodeID,
    Stmt,
};

static NEXT_PROJECT_ID: AtomicUsize = AtomicUsize::new(0);

struct TestProject {
    root: PathBuf,
    entry: PathBuf,
    stdlib: PathBuf,
}

impl TestProject {
    fn new(source: &str) -> Self {
        let id = NEXT_PROJECT_ID.fetch_add(
            1,
            Ordering::Relaxed,
        );

        let root = std::env::temp_dir().join(format!(
            "hydra-analyzer-match-semantics-{}-{id}",
            std::process::id(),
        ));

        let entry = root.join("main.hydra");
        let stdlib = root.join("stdlib");
        let fmt = stdlib
            .join("core")
            .join("fmt");

        fs::create_dir_all(&fmt)
            .expect("failed to create test stdlib");

        fs::write(
            &entry,
            source,
        )
        .expect("failed to write test source");

        fs::write(
            fmt.join("mod.hydra"),
            "",
        )
        .expect("failed to write fmt module");

        Self {
            root,
            entry,
            stdlib,
        }
    }

    fn build(&self) -> (ModuleTree, SourceMap) {
        let mut source_map = SourceMap::new();

        let mut program = ModuleTree::build(
            &self.entry,
            self.stdlib.clone(),
            &mut source_map,
        )
        .expect("module tree should build");

        program
            .resolve_imports(&mut source_map)
            .expect("imports should resolve");

        (
            program,
            source_map,
        )
    }
}

impl Drop for TestProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(
            &self.root,
        );
    }
}

fn root_items(program: &ModuleTree) -> &[Item] {
    program
        .parsed_files
        .values()
        .find_map(|(module_path, items)| {
            if module_path.is_empty() {
                Some(items.as_slice())
            } else {
                None
            }
        })
        .expect("root module should exist")
}

fn function<'a>(
    program: &'a HIRProgram,
    name: &str,
) -> &'a HIRFunction {
    program
        .functions
        .iter()
        .find(|function| function.name == name)
        .unwrap_or_else(|| {
            panic!("function `{name}` should exist")
        })
}

fn local_type(
    function: &HIRFunction,
    context: &HIRContext,
    name: &str,
) -> Type {
    for stmt in &function.body.stmts {
        let HIRStmt::VarDecl {
            def_id,
            ty,
            ..
        } = stmt
        else {
            continue;
        };

        let Some(info) = context.get_def(*def_id) else {
            continue;
        };

        if info.name == name {
            return ty.clone();
        }
    }

    panic!("local `{name}` should exist");
}

fn binding_type(
    context: &HIRContext,
    def_id: DefID,
) -> Type {
    let info = context
        .get_def(def_id)
        .expect("binding definition should exist");

    match &info.kind {
        DefKind::Variable { ty, .. } => ty.clone(),

        other => {
            panic!(
                "expected variable binding, got {other:?}"
            )
        }
    }
}

fn else_binding_id(
    program: &ModuleTree,
    function_name: &str,
) -> NodeID {
    for item in root_items(program) {
        let Item::Function(function) = item else {
            continue;
        };

        if function.name.lexeme != function_name {
            continue;
        }

        let body = function
            .body
            .as_ref()
            .expect("function should have a body");

        for stmt in &body.statements {
            let Stmt::VariableDecl {
                initializer,
                ..
            } = stmt
            else {
                continue;
            };

            let Expr::Match {
                else_arm: Some(else_arm),
                ..
            } = &**initializer
            else {
                continue;
            };

            let (id, _) = else_arm
                .binding
                .as_ref()
                .expect("else arm should have a binding");

            return *id;
        }
    }

    panic!(
        "function `{function_name}` should contain an else binding"
    );
}

fn analyze(
    source: &str,
    binding_function: Option<&str>,
) -> (
    HIRProgram,
    HIRContext,
    Option<DefID>,
) {
    let project = TestProject::new(source);
    let (program, source_map) = project.build();

    let binding_id = binding_function.map(|name| {
        else_binding_id(
            &program,
            name,
        )
    });

    let mut context = HIRContext::default();

    let resolver = Resolver::new(
        &program,
        &mut context,
        &source_map,
    );

    let (name_resolver, global_symbols) = resolver
        .resolve()
        .expect("name resolution should succeed");

    let binding_def = binding_id.map(|id| {
        name_resolver
            .get_resolution(id)
            .expect("binding should resolve")
    });

    let analyzer = Analyzer::new(
        &program,
        &mut context,
        &source_map,
        name_resolver,
        global_symbols,
    );

    let hir = analyzer
        .analyze()
        .expect("semantic analysis should succeed");

    (
        hir,
        context,
        binding_def,
    )
}

#[test]
fn match_works_on_non_optional_values() {
    let (hir, context, _) = analyze(
        r#"
        fn select(x: i32) -> i32 {
            let y = match x {
                1: value => {
                    value + 1;
                }
            } else: remaining {
                remaining;
            };

            return y;
        }

        fn main() -> void {
        }
        "#,
        None,
    );

    assert_eq!(
        local_type(
            function(&hir, "select"),
            &context,
            "y",
        ),
        Type::I32,
    );
}

#[test]
fn match_result_remains_optional_when_else_returns_optional_binding() {
    let (
        hir,
        context,
        binding_def,
    ) = analyze(
        r#"
        fn keep_optional(x: i32?) -> i32? {
            let y = match x {
                1: value => {
                    value + 1;
                }
            } else: remaining {
                remaining;
            };

            return y;
        }

        fn main() -> void {
        }
        "#,
        Some("keep_optional"),
    );

    let optional_i32 = Type::OPTIONAL(
        Box::new(Type::I32),
    );

    assert_eq!(
        local_type(
            function(&hir, "keep_optional"),
            &context,
            "y",
        ),
        optional_i32,
    );

    assert_eq!(
        binding_type(
            &context,
            binding_def.expect("else binding should exist"),
        ),
        Type::OPTIONAL(Box::new(Type::I32)),
    );
}

#[test]
fn optional_else_binding_does_not_force_optional_match_result() {
    let (
        hir,
        context,
        binding_def,
    ) = analyze(
        r#"
        fn normalize(value: i32?) -> i32 {
            return value orelse 0;
        }

        fn eliminate_optional(x: i32?) -> i32 {
            let y = match x {
                1: value => {
                    value + 1;
                }
            } else: remaining {
                normalize(remaining);
            };

            return y;
        }

        fn main() -> void {
        }
        "#,
        Some("eliminate_optional"),
    );

    assert_eq!(
        binding_type(
            &context,
            binding_def.expect("else binding should exist"),
        ),
        Type::OPTIONAL(Box::new(Type::I32)),
    );

    assert_eq!(
        local_type(
            function(&hir, "eliminate_optional"),
            &context,
            "y",
        ),
        Type::I32,
    );
}

#[test]
fn explicit_none_arm_refines_else_binding_to_underlying_type() {
    let (
        hir,
        context,
        binding_def,
    ) = analyze(
        r#"
        fn eliminate_none(x: i32?) -> i32 {
            let y = match x {
                None => {
                    0;
                }

                1: value => {
                    value;
                }
            } else: remaining {
                remaining;
            };

            return y;
        }

        fn main() -> void {
        }
        "#,
        Some("eliminate_none"),
    );

    assert_eq!(
        binding_type(
            &context,
            binding_def.expect("else binding should exist"),
        ),
        Type::I32,
    );

    assert_eq!(
        local_type(
            function(&hir, "eliminate_none"),
            &context,
            "y",
        ),
        Type::I32,
    );
}
