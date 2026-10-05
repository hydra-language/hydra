use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};

use analyzer::{traits, Analyzer, Resolver};
use errors::error::HydraError;
use ir::{
    context::HIRContext,
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
            "hydrac-deref-mut-projection-test-{}-{}",
            std::process::id(),
            id,
        ));

        fs::create_dir_all(dir.join("core/fmt"))
            .expect("failed to create core::fmt test module");

        fs::create_dir_all(dir.join("core/ops"))
            .expect("failed to create core::ops test module");

        fs::write(
            dir.join("core/fmt/mod.hydra"),
            "",
        )
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
        )
        .expect("failed to write Deref test module");

        let entry = dir.join("main.hydra");

        fs::write(
            &entry,
            source,
        )
        .expect("failed to write temporary Hydra source");

        Self { dir, entry }
    }
}

impl Drop for TestProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn analyze_result(
    source: &str,
) -> Result<(HIRProgram, HIRContext), Vec<HydraError>> {
    let project = TestProject::new(source);
    let mut source_map = SourceMap::new();

    let mut module_tree = ModuleTree::build(
        Path::new(&project.entry),
        project.dir.clone(),
        &mut source_map,
    )
    .unwrap_or_else(|errors| {
        panic!("module construction failed:\n{errors:#?}");
    });

    module_tree
        .resolve_imports(&mut source_map)
        .unwrap_or_else(|errors| {
            panic!("import resolution failed:\n{errors:#?}");
        });

    module_tree
        .parse_bodies(&source_map)
        .unwrap_or_else(|errors| {
            panic!("body parsing failed:\n{errors:#?}");
        });

    let mut context = HIRContext::default();

    let resolver = Resolver::new(
        &module_tree,
        &mut context,
        &source_map,
    );

    let (name_resolver, global_symbols) = resolver
        .resolve()
        .unwrap_or_else(|errors| {
            panic!("name resolution failed:\n{errors:#?}");
        });

    let hir = Analyzer::new(
        &module_tree,
        &mut context,
        &source_map,
        name_resolver,
        global_symbols,
    )
    .analyze()?;

    Ok((hir, context))
}

fn analyze(source: &str) -> (HIRProgram, HIRContext) {
    analyze_result(source).unwrap_or_else(|errors| {
        panic!("semantic analysis failed:\n{errors:#?}");
    })
}

fn find_function<'a>(
    hir: &'a HIRProgram,
    name: &str,
) -> &'a HIRFunction {
    hir.functions
        .iter()
        .find(|function| function.name == name)
        .unwrap_or_else(|| {
            panic!("function `{name}` should exist");
        })
}

fn assignment_target(function: &HIRFunction) -> &HIRExpr {
    let stmt = function
        .body
        .stmts
        .first()
        .expect("function should contain an assignment");

    let HIRStmt::Expr(expr) = stmt else {
        panic!(
            "expected expression statement, found {stmt:#?}"
        );
    };

    let HIRExprKind::Assign {
        target,
        ..
    } = &expr.kind
    else {
        panic!(
            "expected assignment, found {:#?}",
            expr.kind,
        );
    };

    target
}

fn assert_contains_deref_mut_call(
    expr: &HIRExpr,
    deref_mut_method: ir::context::DefID,
) {
    match &expr.kind {
        HIRExprKind::Call {
            callee,
            ..
        } if *callee == deref_mut_method => {}

        HIRExprKind::Dereference {
            target,
        } => {
            assert_contains_deref_mut_call(
                target,
                deref_mut_method,
            );
        }

        HIRExprKind::FieldAccess {
            object,
            ..
        } => {
            assert_contains_deref_mut_call(
                object,
                deref_mut_method,
            );
        }

        HIRExprKind::ArrayAccess {
            array,
            ..
        } => {
            assert_contains_deref_mut_call(
                array,
                deref_mut_method,
            );
        }

        _ => {
            panic!(
                "mutable projection did not contain DerefMut::deref_mut: {expr:#?}"
            );
        }
    }
}

#[test]
fn field_assignment_through_deref_uses_deref_mut() {
    let (hir, context) = analyze(
        r#"
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

fn main() -> void {
}
"#,
    );

    let deref_mut_trait = context
        .find_trait_by_path(traits::DEREF_MUT_TRAIT_PATH)
        .expect("DerefMut trait should exist");

    let deref_mut_method = context
        .trait_impls
        .iter()
        .find(|trait_impl| {
            trait_impl.trait_def_id == deref_mut_trait
        })
        .and_then(|trait_impl| {
            trait_impl.methods.get("deref_mut")
        })
        .copied()
        .expect("DerefMut::deref_mut should exist");

    let target = assignment_target(
        find_function(
            &hir,
            "write",
        ),
    );

    assert_contains_deref_mut_call(
        target,
        deref_mut_method,
    );
}

#[test]
fn nested_array_assignment_through_deref_uses_deref_mut() {
    let (hir, context) = analyze(
        r#"
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

fn main() -> void {
}
"#,
    );

    let deref_mut_trait = context
        .find_trait_by_path(traits::DEREF_MUT_TRAIT_PATH)
        .expect("DerefMut trait should exist");

    let deref_mut_method = context
        .trait_impls
        .iter()
        .find(|trait_impl| {
            trait_impl.trait_def_id == deref_mut_trait
        })
        .and_then(|trait_impl| {
            trait_impl.methods.get("deref_mut")
        })
        .copied()
        .expect("DerefMut::deref_mut should exist");

    let target = assignment_target(
        find_function(
            &hir,
            "write",
        ),
    );

    assert_contains_deref_mut_call(
        target,
        deref_mut_method,
    );
}

#[test]
fn deref_mut_cannot_borrow_immutable_owned_binding() {
    let errors = analyze_result(
        r#"
include core::ops::deref::{Deref, DerefMut};

struct Wrapper {
    value: i32;
}

extension Deref<i32> on Wrapper {
    fn deref(&self) -> &i32 {
        return &self.value;
    }
}

extension DerefMut<i32> on Wrapper {
    fn deref_mut(&mut self) -> &mut i32 {
        return &mut self.value;
    }
}

fn main() -> void {
    const wrapper = Wrapper {
        .value = 1,
    };

    *wrapper = 2;
}
"#,
    )
    .expect_err(
        "mutable dereference of an immutable owned binding should fail",
    );

    assert!(
        errors.iter().any(|error| {
            error.code == "S009" &&
            error.message.contains(
                "cannot assign to immutable binding",
            )
        }),
        "expected immutable binding diagnostic, found {errors:#?}",
    );
}
