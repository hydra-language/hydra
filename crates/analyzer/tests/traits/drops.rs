use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};

use analyzer::{Analyzer, Resolver};
use errors::error::HydraError;
use ir::context::SemanticContext;
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
            "hydrac-drop-rules-test-{}-{id}",
            std::process::id(),
        ));

        fs::create_dir_all(dir.join("core/ops"))
            .expect("failed to create core::ops test module");

        fs::write(
            dir.join("core/ops/drop.hydra"),
            r#"
pub trait Drop {
    fn drop(&mut self) -> void;
}
"#,
        ).expect("failed to write Drop trait");

        let entry = dir.join("main.hydra");
        let source = format!(r#"
include core::ops::drop::Drop;

{source}
"#);

        fs::write(&entry, source).expect("failed to write temporary Hydra source");
        Self { dir, entry }
    }
}

impl Drop for TestProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn analyze_result(source: &str) -> Result<SemanticContext, Vec<HydraError>> {
    let project = TestProject::new(source);
    let mut source_map = SourceMap::new();

    let mut module_tree = ModuleTree::build(
        Path::new(&project.entry),
        project.dir.clone(),
        &mut source_map,
    ).unwrap_or_else(|errors| {
        panic!("module construction failed:\n{errors:#?}");
    });

    module_tree.resolve_imports(&mut source_map).unwrap_or_else(|errors| {
        panic!("import resolution failed:\n{errors:#?}");
    });

    module_tree.parse_bodies(&source_map).unwrap_or_else(|errors| {
        panic!("body parsing failed:\n{errors:#?}");
    });

    let mut context = SemanticContext::new();
    let (name_resolver, global_symbols) =
        Resolver::new(&module_tree, &mut context, &source_map)
            .resolve()
            .unwrap_or_else(|errors| {
                panic!("name resolution failed:\n{errors:#?}");
            });

    Analyzer::new(
        &module_tree,
        context,
        &source_map,
        name_resolver,
        global_symbols,
    )
    .analyze()
    .map(|analyzed| analyzed.context)
}

fn analyze(source: &str) {
    analyze_result(source).unwrap_or_else(|errors| {
        panic!("semantic analysis failed:\n{errors:#?}");
    });
}

fn assert_has_error(source: &str, code: &str) {
    let errors = match analyze_result(source) {
        Ok(_) => panic!("analysis should have failed with {code}"),
        Err(errors) => errors,
    };

    assert!(
        errors.iter().any(|error| error.code == code),
        "expected error {code}, found:\n{errors:#?}",
    );
}

#[test]
fn drop_trait_implementation_is_allowed() {
    analyze(r#"
struct Resource;

extension Drop on Resource {
    fn drop(&mut self) -> void {
    }
}

fn main() -> void {
    let resource = Resource {};
}
"#);
}

#[test]
fn inherent_method_named_drop_is_rejected() {
    assert_has_error(r#"
struct Resource;

extension Resource {
    fn drop(&mut self) -> void {
    }
}

fn main() -> void {
}
"#, "S026");
}

#[test]
fn unrelated_trait_cannot_declare_drop_method() {
    assert_has_error(r#"
trait Cleanup {
    fn drop(&mut self) -> void;
}

fn main() -> void {
}
"#, "S026");
}

#[test]
fn drop_trait_method_cannot_be_called_explicitly() {
    assert_has_error(r#"
struct Resource;

extension Drop on Resource {
    fn drop(&mut self) -> void {
    }
}

fn main() -> void {
    let resource = Resource {};
    resource::drop();
}
"#, "S027");
}
