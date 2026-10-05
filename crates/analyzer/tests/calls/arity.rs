use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};

use analyzer::{Analyzer, Resolver};
use errors::error::HydraError;
use ir::{context::HIRContext, hir::HIRProgram};
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
            "hydrac-arity-test-{}-{}",
            std::process::id(),
            id,
        ));

        fs::create_dir_all(&dir).expect("failed to create temporary test directory");

        let entry = dir.join("main.hydra");
        fs::write(&entry, source).expect("failed to write temporary Hydra source");

        Self { dir, entry }
    }

    fn write(&self, relative: &str, source: &str) {
        let path = self.dir.join(relative);

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("failed to create temporary module directory");
        }

        fs::write(path, source).expect("failed to write temporary module");
    }
}

impl Drop for TestProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn analyze_project(project: &TestProject) -> Result<HIRProgram, Vec<HydraError>> {
    let mut source_map = SourceMap::new();

    let mut module_tree = ModuleTree::build(
        Path::new(&project.entry),
        project.dir.clone(),
        &mut source_map,
    )
    .unwrap_or_else(|errors| {
        panic!("module construction failed:\n{errors:#?}");
    });

    module_tree.resolve_imports(&mut source_map).unwrap_or_else(|errors| {
        panic!("import resolution failed:\n{errors:#?}");
    });

    module_tree.parse_bodies(&source_map).unwrap_or_else(|errors| {
        panic!("body parsing failed:\n{errors:#?}");
    });

    let mut context = HIRContext::default();
    let resolver = Resolver::new(&module_tree, &mut context, &source_map);

    let (name_resolver, global_symbols) = resolver.resolve().unwrap_or_else(|errors| {
        panic!("name resolution failed:\n{errors:#?}");
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

fn analyze(source: &str) -> Result<HIRProgram, Vec<HydraError>> {
    let project = TestProject::new(source);
    analyze_project(&project)
}

fn assert_arity_error(source: &str, message: &str) {
    let errors = analyze(source).expect_err("call with incorrect arity should fail");

    assert!(
        errors.iter().any(|error| error.code == "S018" && error.message == message),
        "expected S018 `{message}`, got {errors:#?}",
    );
}

fn assert_project_arity_error(project: &TestProject, message: &str) {
    let errors = analyze_project(project).expect_err("call with incorrect arity should fail");

    assert!(
        errors.iter().any(|error| error.code == "S018" && error.message == message),
        "expected S018 `{message}`, got {errors:#?}",
    );
}

#[test]
fn rejects_too_few_function_arguments() {
    assert_arity_error(
        r#"
fn add(left: i32, right: i32) -> i32 {
    return left + right;
}

fn main() -> void {
    add(1);
}
"#,
        "function `add` expected 2 arguments, found 1",
    );
}

#[test]
fn rejects_too_many_function_arguments() {
    assert_arity_error(
        r#"
fn add(left: i32, right: i32) -> i32 {
    return left + right;
}

fn main() -> void {
    add(1, 2, 3);
}
"#,
        "function `add` expected 2 arguments, found 3",
    );
}

#[test]
fn rejects_too_few_generic_function_arguments() {
    assert_arity_error(
        r#"
fn select<T>(left: T, right: T) -> T {
    return left;
}

fn main() -> void {
    select::<i32>(1);
}
"#,
        "function `select` expected 2 arguments, found 1",
    );
}

#[test]
fn rejects_too_many_generic_function_arguments() {
    assert_arity_error(
        r#"
fn identity<T>(value: T) -> T {
    return value;
}

fn main() -> void {
    identity::<i32>(1, 2);
}
"#,
        "function `identity` expected 1 argument, found 2",
    );
}

#[test]
fn rejects_too_few_method_arguments() {
    assert_arity_error(
        r#"
struct Value {
    value: i32;
}

extension Value {
    fn add(&self, amount: i32) -> i32 {
        return self.value + amount;
    }
}

fn use_value(value: Value) -> i32 {
    return value::add();
}

fn main() -> void {
}
"#,
        "method `add` expected 1 argument, found 0",
    );
}

#[test]
fn rejects_too_many_method_arguments() {
    assert_arity_error(
        r#"
struct Value {
    value: i32;
}

extension Value {
    fn add(&self, amount: i32) -> i32 {
        return self.value + amount;
    }
}

fn use_value(value: Value) -> i32 {
    return value::add(1, 2);
}

fn main() -> void {
}
"#,
        "method `add` expected 1 argument, found 2",
    );
}

#[test]
fn explicit_receiver_dispatch_counts_receiver_as_argument() {
    assert_arity_error(
        r#"
struct Value {
    value: i32;
}

extension Value {
    fn add(&self, amount: i32) -> i32 {
        return self.value + amount;
    }
}

fn use_value(value: Value) -> i32 {
    return Value::add(value);
}

fn main() -> void {
}
"#,
        "function `Value::add` expected 2 arguments, found 1",
    );
}

#[test]
fn rejects_too_few_intrinsic_arguments() {
    let project = TestProject::new(
        r#"
include core::intrinsics;

fn main() -> void {
    intrinsics::__ptr_read::<i32>();
}
"#,
    );

    project.write(
        "core/intrinsics.hydra",
        r#"
#[intrinsic]
pub fn __ptr_read<T>(ptr: *const T) -> T;
"#,
    );

    assert_project_arity_error(
        &project,
        "function `intrinsics::__ptr_read` expected 1 argument, found 0",
    );
}

#[test]
fn rejects_too_many_intrinsic_arguments() {
    let project = TestProject::new(
        r#"
include core::intrinsics;

fn main() -> void {
    intrinsics::__ptr_read::<i32>(0, 1);
}
"#,
    );

    project.write(
        "core/intrinsics.hydra",
        r#"
#[intrinsic]
pub fn __ptr_read<T>(ptr: *const T) -> T;
"#,
    );

    assert_project_arity_error(
        &project,
        "function `intrinsics::__ptr_read` expected 1 argument, found 2",
    );
}

#[test]
fn accepts_exact_function_and_method_arity() {
    analyze(
        r#"
struct Value {
    value: i32;
}

extension Value {
    fn add(&self, amount: i32) -> i32 {
        return self.value + amount;
    }
}

fn sum(left: i32, right: i32) -> i32 {
    return left + right;
}

fn use_value(value: Value) -> i32 {
    const left = value::add(1);
    const right = Value::add(value, 2);

    return sum(left, right);
}

fn main() -> void {
}
"#,
    )
    .expect("calls with exact arity should analyze");
}
