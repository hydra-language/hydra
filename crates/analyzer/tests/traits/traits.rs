use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

use analyzer::{Analyzer, Resolver};
use errors::error::HydraError;
use ir::{
    context::{DefID, DefKind, SemanticContext},
    types::Type,
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
            "hydrac-trait-test-{}-{id}",
            std::process::id(),
        ));

        fs::create_dir_all(&dir).expect("failed to create test directory");
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

fn analyze_result(source: &str) -> Result<SemanticContext, Vec<HydraError>> {
    let project = TestProject::new(source);
    let mut source_map = SourceMap::new();
    let mut modules = ModuleTree::build(
        &project.entry,
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

    Ok(analyzed.context)
}

fn analyze(source: &str) -> SemanticContext {
    analyze_result(source).unwrap_or_else(|errors| {
        panic!("semantic analysis failed:\n{errors:#?}");
    })
}

fn find_definition(context: &SemanticContext, name: &str) -> DefID {
    context.definitions.iter()
        .find_map(|(id, info)| (info.name == name).then_some(*id))
        .unwrap_or_else(|| panic!("definition `{name}` should exist"))
}

fn assert_has_error(source: &str, code: &str) {
    let errors = analyze_result(source)
        .expect_err("semantic analysis should have failed");

    assert!(
        errors.iter().any(|error| error.code == code),
        "expected error {code}, found:\n{errors:#?}",
    );
}

#[test]
fn trait_is_stored_as_a_trait_definition() {
    let context = analyze(r#"
trait Display {
    fn fmt(&self, value: usize) -> void;
}

fn main() -> void {}
"#);

    let id = find_definition(&context, "Display");
    let definition = context.get_def(id).expect("Display should exist");

    let DefKind::Trait { methods, .. } = &definition.kind else {
        panic!("Display should be a trait definition");
    };

    assert_eq!(methods.len(), 1);
}

#[test]
fn trait_method_signature_uses_generic_self() {
    let context = analyze(r#"
trait Measure {
    fn measure(&self, scale: usize) -> usize;
}

fn main() -> void {}
"#);

    let id = find_definition(&context, "Measure");
    let definition = context.get_def(id).expect("Measure should exist");

    let DefKind::Trait { methods, .. } = &definition.kind else {
        panic!("Measure should be a trait");
    };

    let method = context.get_def(methods[0]).expect("measure method should exist");
    assert_eq!(method.absolute_path, vec!["Measure", "measure"]);

    let DefKind::Function { params, return_type, .. } = &method.kind else {
        panic!("measure should be a function");
    };

    assert_eq!(
        params,
        &vec![
            Type::CONST_REF(Box::new(Type::GENERIC("Self".into()))),
            Type::USIZE,
        ],
    );
    assert_eq!(return_type, &Type::USIZE);
}

#[test]
fn mutable_trait_receiver_preserves_mutability() {
    let context = analyze(r#"
trait Writer {
    fn write(&mut self, value: u8) -> void;
}

fn main() -> void {}
"#);

    let id = find_definition(&context, "Writer");
    let definition = context.get_def(id).expect("Writer should exist");

    let DefKind::Trait { methods, .. } = &definition.kind else {
        panic!("Writer should be a trait");
    };

    let method = context.get_def(methods[0]).expect("write should exist");
    let DefKind::Function { params, .. } = &method.kind else {
        panic!("write should be a function");
    };

    assert_eq!(
        params,
        &vec![
            Type::REF(Box::new(Type::GENERIC("Self".into()))),
            Type::U8,
        ],
    );
}

#[test]
fn trait_implementation_is_registered() {
    let context = analyze(r#"
trait Display {
    fn fmt(&self) -> void;
}

struct Thing {}

extension Display on Thing {
    fn fmt(&self) -> void {}
}

fn main() -> void {}
"#);

    let display_id = find_definition(&context, "Display");
    let thing_id = find_definition(&context, "Thing");

    let implementation = context.trait_impls.iter()
        .find(|implementation| {
            implementation.trait_def_id == display_id
                && matches!(
                    &implementation.target_type,
                    Type::STRUCT(type_ref) if type_ref.def_id == thing_id
                )
        })
        .expect("Display implementation for Thing should exist");

    let method_id = implementation.methods.get("fmt")
        .expect("implementation should contain fmt");
    let method = context.get_def(*method_id).expect("fmt definition should exist");

    assert_eq!(method.absolute_path.join("::"), "Thing::<Display>::fmt");
}

#[test]
fn trait_self_return_type_is_substituted_with_implementing_type() {
    analyze(r#"
trait Factory {
    fn make(&self) -> Self;
}

struct Thing {}

extension Factory on Thing {
    fn make(&self) -> Thing {
        return Thing {};
    }
}

fn main() -> void {}
"#);
}

#[test]
fn trait_implementation_must_provide_required_methods() {
    assert_has_error(r#"
trait Display {
    fn fmt(&self) -> void;
}

struct Thing {}

extension Display on Thing {}

fn main() -> void {}
"#, "S021");
}

#[test]
fn trait_implementation_method_signature_must_match_contract() {
    assert_has_error(r#"
trait Writer {
    fn write(&mut self, value: u8) -> void;
}

struct Thing {}

extension Writer on Thing {
    fn write(&self, value: u8) -> void {}
}

fn main() -> void {}
"#, "S022");
}

#[test]
fn trait_implementation_method_parameter_types_must_match_contract() {
    assert_has_error(r#"
trait Writer {
    fn write(&mut self, value: u8) -> void;
}

struct Thing {}

extension Writer on Thing {
    fn write(&mut self, value: u16) -> void {}
}

fn main() -> void {}
"#, "S022");
}

#[test]
fn duplicate_trait_implementations_are_rejected() {
    assert_has_error(r#"
trait Display {
    fn fmt(&self) -> void;
}

struct Thing {}

extension Display on Thing {
    fn fmt(&self) -> void {}
}

extension Display on Thing {
    fn fmt(&self) -> void {}
}

fn main() -> void {}
"#, "S023");
}

#[test]
fn extension_trait_target_must_actually_be_a_trait() {
    assert_has_error(r#"
struct NotATrait {}
struct Thing {}

extension NotATrait on Thing {}

fn main() -> void {}
"#, "S020");
}

#[test]
fn trait_method_can_be_called_with_receiver_syntax() {
    analyze(r#"
trait Speak {
    fn speak(&self) -> void;
}

struct Thing {}

extension Speak on Thing {
    fn speak(&self) -> void {}
}

fn main() -> void {
    let thing = Thing {};
    thing::speak();
}
"#);
}

#[test]
fn inherent_method_takes_precedence_over_trait_method() {
    analyze(r#"
trait Describe {
    fn kind(&self) -> u16;
}

struct Thing {}

extension Thing {
    fn kind(&self) -> u8 {
        return 1;
    }
}

extension Describe on Thing {
    fn kind(&self) -> u16 {
        return 2;
    }
}

fn main() -> void {
    let thing = Thing {};
    let value: u8 = thing::kind();
}
"#);
}

#[test]
fn ambiguous_trait_methods_are_rejected() {
    assert_has_error(r#"
trait First {
    fn run(&self) -> void;
}

trait Second {
    fn run(&self) -> void;
}

struct Thing {}

extension First on Thing {
    fn run(&self) -> void {}
}

extension Second on Thing {
    fn run(&self) -> void {}
}

fn main() -> void {
    let thing = Thing {};
    thing::run();
}
"#, "S024");
}

#[test]
fn generic_trait_implementation_matches_concrete_generic_type() {
    analyze(r#"
trait Touch {
    fn touch(&self) -> void;
}

struct Wrapper<T> {
    value: T;
}

extension<T> Touch on Wrapper<T> {
    fn touch(&self) -> void {}
}

fn touch_wrapper(wrapper: Wrapper<i32>) -> void {
    wrapper::touch();
}

fn main() -> void {}
"#);
}
