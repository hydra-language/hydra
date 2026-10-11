use std::{fs, path::PathBuf, sync::atomic::{AtomicUsize, Ordering}};

use analyzer::Resolver;
use ir::context::SemanticContext;
use parser::{
    module::{ModuleTree, SourceMap},
    Expr, Item, NodeID, Stmt,
};

static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

struct Project {
    root: PathBuf,
    entry: PathBuf,
    stdlib: PathBuf,
}

impl Project {
    fn new(source: &str) -> Self {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "hydra-match-resolution-{}-{id}", std::process::id()
        ));
        let entry = root.join("main.hydra");
        let stdlib = root.join("stdlib");
        fs::create_dir_all(stdlib.join("core/fmt")).expect("create stdlib");
        fs::write(stdlib.join("core/fmt/mod.hydra"), "").expect("write fmt module");
        fs::write(&entry, source).expect("write source");
        Self { root, entry, stdlib }
    }

    fn build(&self) -> (ModuleTree, SourceMap) {
        let mut source_map = SourceMap::new();
        let mut modules = ModuleTree::build(&self.entry, self.stdlib.clone(), &mut source_map)
            .expect("module tree should build");
        modules.resolve_imports(&mut source_map).expect("imports should resolve");
        (modules, source_map)
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn main_function(program: &ModuleTree) -> &parser::FunctionDecl {
    program.parsed_files.iter()
        .filter(|(_, (path, _))| path.is_empty())
        .flat_map(|(_, (_, items))| items)
        .find_map(|item| match item {
            Item::Function(function) if function.name.lexeme == "main" => Some(function),
            _ => None,
        })
        .expect("main should exist")
}

fn match_binding_and_usage_ids(program: &ModuleTree) -> (NodeID, NodeID, NodeID, NodeID) {
    let function = main_function(program);
    let body = function.body.as_ref().expect("main should have a body");

    let Stmt::Expr(expr) = &body.statements[0] else {
        panic!("expected match expression");
    };
    let Expr::Match { arms, else_arm, .. } = &**expr else {
        panic!("expected match expression");
    };

    let (arm_id, _) = arms[0].binding.as_ref().expect("expected arm binding");
    let Stmt::Expr(arm_usage) = &arms[0].body.statements[0] else {
        panic!("expected arm binding usage");
    };
    let Expr::Variable { id: arm_usage_id, .. } = &**arm_usage else {
        panic!("expected arm variable");
    };

    let else_arm = else_arm.as_ref().expect("expected else arm");
    let (else_id, _) = else_arm.binding.as_ref().expect("expected else binding");
    let Stmt::Expr(else_usage) = &else_arm.body.statements[0] else {
        panic!("expected else binding usage");
    };
    let Expr::Variable { id: else_usage_id, .. } = &**else_usage else {
        panic!("expected else variable");
    };

    (*arm_id, *arm_usage_id, *else_id, *else_usage_id)
}

#[test]
fn match_bindings_resolve_inside_their_branch() {
    let project = Project::new(r#"
        fn main(x: i32) -> void {
            match x {
                1: value => { value; }
            } else: remaining {
                remaining;
            }
        }
    "#);
    let (program, source_map) = project.build();
    let (arm_id, arm_usage, else_id, else_usage) = match_binding_and_usage_ids(&program);

    let mut context = SemanticContext::new();
    let (resolver, _) = Resolver::new(&program, &mut context, &source_map)
        .resolve()
        .expect("match bindings should resolve");

    let arm_def = resolver.get_resolution(arm_id).expect("arm binding should resolve");
    let else_def = resolver.get_resolution(else_id).expect("else binding should resolve");

    assert_eq!(arm_def, resolver.get_resolution(arm_usage).expect("arm use should resolve"));
    assert_eq!(else_def, resolver.get_resolution(else_usage).expect("else use should resolve"));
    assert_ne!(arm_def, else_def, "match branches must have separate binding definitions");
}

#[test]
fn match_binding_does_not_escape_its_branch() {
    let project = Project::new(r#"
        fn main(x: i32) -> void {
            match x {
                1: value => { value; }
            } else {}
            value;
        }
    "#);
    let (program, source_map) = project.build();
    let mut context = SemanticContext::new();

    let errors = Resolver::new(&program, &mut context, &source_map)
        .resolve()
        .expect_err("match binding should not escape its branch");

    assert!(
        errors.iter().any(|error| {
            error.code == "R001"
                && error.message.contains("cannot find value `value` in this scope")
        }),
        "expected unresolved-name diagnostic, got {errors:#?}",
    );
}
