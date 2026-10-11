use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};

use analyzer::{traits, Analyzer, Resolver};
use errors::error::HydraError;
use ir::{
    context::{DefID, SemanticContext},
    hir::{HIRExpr, HIRExprKind, HIRFunction, HIRProgram, HIRStmt},
};
use parser::module::{ModuleTree, SourceMap};

static NEXT_TEST_ID: AtomicUsize = AtomicUsize::new(0);

struct TestProject {
    dir: PathBuf,
    entry: PathBuf,
}

impl TestProject {
    fn new(source: &str) -> Self {
        let id = NEXT_TEST_ID.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "hydrac-deref-mut-projection-test-{}-{id}",
            std::process::id(),
        ));

        fs::create_dir_all(dir.join("core/fmt"))
            .expect("failed to create core::fmt test module");
        fs::create_dir_all(dir.join("core/ops"))
            .expect("failed to create core::ops test module");

        fs::write(dir.join("core/fmt/mod.hydra"), "")
            .expect("failed to write core::fmt test module");

        fs::write(
            dir.join("core/ops/deref.hydra"),
            r#"
pub trait Deref<Target> {
    fn deref(&self) -> &Target;
}

pub trait DerefMut<Target> {
    fn deref_mut(&mut self) -> &mut Target;
}
"#,
        ).expect("failed to write Deref test module");

        let entry = dir.join("main.hydra");
        fs::write(&entry, source).expect("failed to write Hydra source");

        Self { dir, entry }
    }
}

impl Drop for TestProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn analyze_result(source: &str) -> Result<(HIRProgram, SemanticContext), Vec<HydraError>> {
    let project = TestProject::new(source);
    let mut source_map = SourceMap::new();

    let mut modules = ModuleTree::build(
        Path::new(&project.entry),
        project.dir.clone(),
        &mut source_map,
    )?;

    modules.resolve_imports(&mut source_map)?;
    modules.parse_bodies(&source_map)?;

    let mut context = SemanticContext::new();
    let (name_resolver, global_symbols) =
        Resolver::new(&modules, &mut context, &source_map).resolve()?;

    let analyzed = Analyzer::new(
        &modules,
        context,
        &source_map,
        name_resolver,
        global_symbols,
    ).analyze()?;

    Ok((analyzed.hir, analyzed.context))
}

fn analyze(source: &str) -> (HIRProgram, SemanticContext) {
    analyze_result(source).unwrap_or_else(|errors| {
        panic!("semantic analysis failed:\n{errors:#?}");
    })
}

fn find_function<'a>(hir: &'a HIRProgram, name: &str) -> &'a HIRFunction {
    hir.functions.iter()
        .find(|function| function.name == name)
        .unwrap_or_else(|| panic!("function `{name}` should exist"))
}

fn assignment_target(function: &HIRFunction) -> &HIRExpr {
    let stmt = function.body.stmts.first().expect("expected assignment");

    let HIRStmt::Expr(expr) = stmt else {
        panic!("expected expression statement, found {stmt:#?}");
    };

    let HIRExprKind::Assign { target, .. } = &expr.kind else {
        panic!("expected assignment, found {:#?}", expr.kind);
    };

    target
}

fn assert_contains_deref_mut_call(expr: &HIRExpr, method: DefID) {
    match &expr.kind {
        HIRExprKind::Call { callee, .. } if callee.def_id == method => {}

        HIRExprKind::Dereference { target } => {
            assert_contains_deref_mut_call(target, method);
        }

        HIRExprKind::FieldAccess { object, .. } => {
            assert_contains_deref_mut_call(object, method);
        }

        HIRExprKind::ArrayAccess { array, .. } => {
            assert_contains_deref_mut_call(array, method);
        }

        _ => panic!("mutable projection did not contain DerefMut::deref_mut: {expr:#?}"),
    }
}

fn assert_mutable_projection(source: &str) {
    let (hir, context) = analyze(source);

    let trait_id = context.find_trait_by_path(traits::DEREF_MUT_TRAIT_PATH)
        .expect("DerefMut trait should exist");

    let method = context.trait_impls.iter()
        .find(|implementation| implementation.trait_def_id == trait_id)
        .and_then(|implementation| implementation.methods.get("deref_mut"))
        .copied()
        .expect("DerefMut::deref_mut should exist");

    let target = assignment_target(find_function(&hir, "write"));
    assert_contains_deref_mut_call(target, method);
}

#[test]
fn field_assignment_through_deref_uses_deref_mut() {
    assert_mutable_projection(r#"
include core::ops::deref::{Deref, DerefMut};

struct Target {
    value: i32;
}

struct Wrapper {
    value: Target;
}

extension Deref<Target> on Wrapper {
    fn deref(&self) -> &Target {
        return &self.value;
    }
}

extension DerefMut<Target> on Wrapper {
    fn deref_mut(&mut self) -> &mut Target {
        return &mut self.value;
    }
}

fn write(wrapper: Wrapper, value: i32) -> void {
    (*wrapper).value = value;
}

fn main() -> void {}
"#);
}

#[test]
fn nested_array_assignment_through_deref_uses_deref_mut() {
    assert_mutable_projection(r#"
include core::ops::deref::{Deref, DerefMut};

struct Target {
    values: [i32, 2];
}

struct Wrapper {
    value: Target;
}

extension Deref<Target> on Wrapper {
    fn deref(&self) -> &Target {
        return &self.value;
    }
}

extension DerefMut<Target> on Wrapper {
    fn deref_mut(&mut self) -> &mut Target {
        return &mut self.value;
    }
}

fn write(wrapper: Wrapper, value: i32) -> void {
    (*wrapper).values[0] = value;
}

fn main() -> void {}
"#);
}
