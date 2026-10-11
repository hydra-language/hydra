use std::{fs, path::PathBuf, sync::atomic::{AtomicUsize, Ordering}};

use analyzer::{Analyzer, Resolver};
use ir::{
    context::{DefID, DefKind, SemanticContext},
    hir::{HIRFunction, HIRProgram, HIRStmt},
    types::Type,
};
use parser::{module::{ModuleTree, SourceMap}, Expr, Item, NodeID, Stmt};

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
            "hydra-match-semantics-{}-{id}", std::process::id()
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

fn root_items(program: &ModuleTree) -> &[Item] {
    program.parsed_files.iter()
        .find_map(|(_, (path, items))| path.is_empty().then_some(items.as_slice()))
        .expect("root module should exist")
}

fn function<'a>(hir: &'a HIRProgram, name: &str) -> &'a HIRFunction {
    hir.functions.iter().find(|function| function.name == name)
        .unwrap_or_else(|| panic!("function `{name}` should exist"))
}

fn local_type(function: &HIRFunction, name: &str) -> Type {
    function.body.stmts.iter().find_map(|stmt| match stmt {
        HIRStmt::VarDecl { local, .. } if local.name == name => Some(local.ty.clone()),
        _ => None,
    }).unwrap_or_else(|| panic!("local `{name}` should exist"))
}

fn binding_type(context: &SemanticContext, id: DefID) -> Type {
    let info = context.get_def(id).expect("binding definition should exist");
    match &info.kind {
        DefKind::Variable { ty, .. } => ty.clone(),
        other => panic!("expected variable binding, found {other:?}"),
    }
}

fn else_binding_id(program: &ModuleTree, function_name: &str) -> NodeID {
    for item in root_items(program) {
        let Item::Function(function) = item else { continue };
        if function.name.lexeme != function_name { continue }

        let body = function.body.as_ref().expect("function should have a body");
        for stmt in &body.statements {
            let Stmt::VariableDecl { initializer, .. } = stmt else { continue };
            let Expr::Match { else_arm: Some(else_arm), .. } = &**initializer else {
                continue;
            };
            let (id, _) = else_arm.binding.as_ref().expect("else arm should have a binding");
            return *id;
        }
    }
    panic!("function `{function_name}` should contain an else binding");
}

fn analyze(source: &str, binding_function: Option<&str>) -> (HIRProgram, SemanticContext, Option<DefID>) {
    let project = Project::new(source);
    let (mut modules, mut source_map) = project.build();
    let binding_id = binding_function.map(|name| else_binding_id(&modules, name));

    modules.parse_bodies(&source_map).expect("function bodies should parse");

    let mut context = SemanticContext::new();
    let (resolver, symbols) = Resolver::new(&modules, &mut context, &source_map)
        .resolve()
        .expect("name resolution should succeed");

    let binding_def = binding_id.map(|id| {
        resolver.get_resolution(id).expect("binding should resolve")
    });

    let analyzed = Analyzer::new(&modules, context, &source_map, resolver, symbols)
        .analyze()
        .expect("semantic analysis should succeed");

    (analyzed.hir, analyzed.context, binding_def)
}

#[test]
fn match_works_on_non_optional_values() {
    let (hir, _, _) = analyze(r#"
        fn select(x: i32) -> i32 {
            let y = match x {
                1: value => { value + 1; }
            } else: remaining {
                remaining;
            };
            return y;
        }
        fn main() -> void {}
    "#, None);

    assert_eq!(local_type(function(&hir, "select"), "y"), Type::I32);
}

#[test]
fn match_result_remains_optional_when_else_returns_optional_binding() {
    let (hir, context, binding_def) = analyze(r#"
        fn keep_optional(x: i32?) -> i32? {
            let y = match x {
                1: value => { value + 1; }
            } else: remaining {
                remaining;
            };
            return y;
        }
        fn main() -> void {}
    "#, Some("keep_optional"));

    let optional = Type::OPTIONAL(Box::new(Type::I32));
    assert_eq!(local_type(function(&hir, "keep_optional"), "y"), optional);
    assert_eq!(binding_type(&context, binding_def.expect("else binding")), optional);
}

#[test]
fn optional_else_binding_does_not_force_optional_match_result() {
    let (hir, context, binding_def) = analyze(r#"
        fn normalize(value: i32?) -> i32 {
            return value orelse 0;
        }

        fn eliminate_optional(x: i32?) -> i32 {
            let y = match x {
                1: value => { value + 1; }
            } else: remaining {
                normalize(remaining);
            };
            return y;
        }

        fn main() -> void {}
    "#, Some("eliminate_optional"));

    assert_eq!(
        binding_type(&context, binding_def.expect("else binding")),
        Type::OPTIONAL(Box::new(Type::I32)),
    );
    assert_eq!(local_type(function(&hir, "eliminate_optional"), "y"), Type::I32);
}

#[test]
fn explicit_none_arm_refines_else_binding_to_underlying_type() {
    let (hir, context, binding_def) = analyze(r#"
        fn eliminate_none(x: i32?) -> i32 {
            let y = match x {
                None => { 0; }
            } else: remaining {
                remaining;
            };
            return y;
        }
        fn main() -> void {}
    "#, Some("eliminate_none"));

    assert_eq!(
        binding_type(&context, binding_def.expect("else binding")),
        Type::I32,
    );
    assert_eq!(local_type(function(&hir, "eliminate_none"), "y"), Type::I32);
}
