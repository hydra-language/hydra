use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};

use analyzer::Resolver;
use ir::context::HIRContext;
use parser::{
    module::{ModuleTree, SourceMap},
    Expr, Item, NodeID, Stmt,
};

static NEXT_PROJECT_ID: AtomicUsize = AtomicUsize::new(0);

struct TestProject {
    root: PathBuf,
    entry: PathBuf,
    stdlib: PathBuf,
}

impl TestProject {
    fn new(source: &str) -> Self {
        let id = NEXT_PROJECT_ID.fetch_add(1, Ordering::Relaxed);

        let root = std::env::temp_dir().join(format!(
            "hydra-analyzer-match-resolution-{}-{id}",
            std::process::id(),
        ));

        let entry = root.join("main.hydra");
        let stdlib = root.join("stdlib");
        let fmt = stdlib.join("core").join("fmt");

        fs::create_dir_all(&fmt).expect("failed to create test stdlib");
        fs::write(&entry, source).expect("failed to write test source");
        fs::write(fmt.join("mod.hydra"), "").expect("failed to write test fmt module");

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

        (program, source_map)
    }
}

impl Drop for TestProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn root_items(program: &ModuleTree) -> &[Item] {
    program
        .parsed_files
        .iter()
        .find_map(|(_, (module_path, items))| {
            if module_path.is_empty() {
                Some(items.as_slice())
            } else {
                None
            }
        })
        .expect("root module should exist")
}

fn main_function(program: &ModuleTree) -> &parser::FunctionDecl {
    root_items(program)
        .iter()
        .find_map(|item| {
            let Item::Function(function) = item else {
                return None;
            };

            if function.name.lexeme == "main" {
                Some(function)
            } else {
                None
            }
        })
        .expect("main function should exist")
}

fn match_binding_and_usage_ids(program: &ModuleTree) -> (NodeID, NodeID, NodeID, NodeID) {
    let function = main_function(program);
    let body = function.body.as_ref().expect("main should have a body");

    let Stmt::Expr(expr) = &body.statements[0] else {
        panic!("expected match expression");
    };

    let Expr::Match {
        arms,
        else_arm,
        ..
    } = &**expr
    else {
        panic!("expected match expression");
    };

    let (arm_binding_id, _) = arms[0]
        .binding
        .as_ref()
        .expect("first arm should have a binding");

    let Stmt::Expr(arm_usage) = &arms[0].body.statements[0] else {
        panic!("expected arm binding usage");
    };

    let Expr::Variable {
        id: arm_usage_id,
        ..
    } = &**arm_usage
    else {
        panic!("expected arm binding variable");
    };

    let else_arm = else_arm.as_ref().expect("expected else arm");

    let (else_binding_id, _) = else_arm
        .binding
        .as_ref()
        .expect("else arm should have a binding");

    let Stmt::Expr(else_usage) = &else_arm.body.statements[0] else {
        panic!("expected else binding usage");
    };

    let Expr::Variable {
        id: else_usage_id,
        ..
    } = &**else_usage
    else {
        panic!("expected else binding variable");
    };

    (
        *arm_binding_id,
        *arm_usage_id,
        *else_binding_id,
        *else_usage_id,
    )
}

#[test]
fn match_bindings_resolve_inside_their_branch() {
    let project = TestProject::new(
        r#"
        fn main(x: i32) -> void {
            match x {
                1: value => {
                    value;
                }
            } else: remaining {
                remaining;
            }
        }
        "#,
    );

    let (program, source_map) = project.build();

    let (
        arm_binding_id,
        arm_usage_id,
        else_binding_id,
        else_usage_id,
    ) = match_binding_and_usage_ids(&program);

    let mut context = HIRContext::default();

    let resolver = Resolver::new(
        &program,
        &mut context,
        &source_map,
    );

    let (name_resolver, _) = resolver
        .resolve()
        .expect("match bindings should resolve");

    let arm_binding_def = name_resolver
        .get_resolution(arm_binding_id)
        .expect("arm binding should have a DefID");

    let arm_usage_def = name_resolver
        .get_resolution(arm_usage_id)
        .expect("arm usage should resolve");

    let else_binding_def = name_resolver
        .get_resolution(else_binding_id)
        .expect("else binding should have a DefID");

    let else_usage_def = name_resolver
        .get_resolution(else_usage_id)
        .expect("else usage should resolve");

    assert_eq!(arm_binding_def, arm_usage_def);
    assert_eq!(else_binding_def, else_usage_def);

    assert_ne!(
        arm_binding_def,
        else_binding_def,
        "each match branch should have its own binding definition",
    );
}

#[test]
fn match_binding_does_not_escape_its_branch() {
    let project = TestProject::new(
        r#"
        fn main(x: i32) -> void {
            match x {
                1: value => {
                    value;
                }
            } else {
            }

            value;
        }
        "#,
    );

    let (program, source_map) = project.build();
    let mut context = HIRContext::default();

    let resolver = Resolver::new(
        &program,
        &mut context,
        &source_map,
    );

    let errors = resolver
        .resolve()
        .expect_err("match binding should not escape its branch");

    assert!(
        errors.iter().any(|error| {
            error.code == "R001" &&
                error.message.contains("cannot find value `value` in this scope")
        }),
        "expected unresolved-name diagnostic for escaped match binding, got: {errors:#?}",
    );
}
