use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};

use analyzer::{Analyzer, Resolver};
use errors::error::HydraError;
use ir::{
    context::{DefID, DefKind, HIRContext},
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
            "hydrac-trait-test-{}-{}",
            std::process::id(),
            id,
        ));

        fs::create_dir_all(&dir).expect("failed to create temporary test directory");

        let entry = dir.join("main.hydra");
        fs::write(&entry, source).expect("failed to write temporary Hydra source");

        Self { dir, entry }
    }
}

impl Drop for TestProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn analyze_result(source: &str) -> Result<HIRContext, Vec<HydraError>> {
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

    let result = Analyzer::new(
        &module_tree,
        &mut context,
        &source_map,
        name_resolver,
        global_symbols,
    )
    .analyze();

    match result {
        Ok(_) => Ok(context),
        Err(errors) => Err(errors),
    }
}

fn analyze(source: &str) -> HIRContext {
    analyze_result(source).unwrap_or_else(|errors| {
        panic!("semantic analysis failed:\n{errors:#?}");
    })
}

fn find_definition(context: &HIRContext, name: &str) -> DefID {
    context
        .definitions
        .iter()
        .find_map(|(def_id, info)| {
            if info.name == name {
                Some(*def_id)
            } else {
                None
            }
        })
        .unwrap_or_else(|| {
            panic!("definition `{name}` should exist");
        })
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
fn trait_is_stored_as_a_trait_definition() {
    let context = analyze(
        r#"
trait Display {
    fn fmt(&self, value: usize) -> void;
}

fn main() -> void {
}
"#,
    );

    let display_def_id = find_definition(&context, "Display");

    let display = context
        .get_def(display_def_id)
        .expect("Display definition should exist");

    let DefKind::Trait { methods } = &display.kind else {
        panic!(
            "Display should be a trait definition, found {:#?}",
            display.kind,
        );
    };

    assert_eq!(
        methods.len(),
        1,
        "Display should contain one contract method",
    );
}

#[test]
fn trait_method_signature_uses_generic_self() {
    let context = analyze(
        r#"
trait Measure {
    fn measure(&self, scale: usize) -> usize;
}

fn main() -> void {
}
"#,
    );

    let measure_trait_def_id = find_definition(&context, "Measure");

    let measure_trait = context
        .get_def(measure_trait_def_id)
        .expect("Measure definition should exist");

    let DefKind::Trait { methods } = &measure_trait.kind else {
        panic!("Measure should be a trait");
    };

    let method = context
        .get_def(methods[0])
        .expect("Measure::measure definition should exist");

    assert_eq!(
        method.absolute_path,
        vec![
            "Measure".to_string(),
            "measure".to_string(),
        ],
    );

    let DefKind::Function {
        params,
        return_type,
        ..
    } = &method.kind
    else {
        panic!("Measure::measure should be a function definition");
    };

    assert_eq!(
        params,
        &vec![
            Type::CONST_REF(Box::new(
                Type::GENERIC("Self".to_string())
            )),
            Type::USIZE,
        ],
    );

    assert_eq!(
        return_type,
        &Type::USIZE,
    );
}

#[test]
fn mutable_trait_receiver_preserves_mutability() {
    let context = analyze(
        r#"
trait Writer {
    fn write(&mut self, value: u8) -> void;
}

fn main() -> void {
}
"#,
    );

    let writer_trait_def_id = find_definition(&context, "Writer");

    let writer_trait = context
        .get_def(writer_trait_def_id)
        .expect("Writer definition should exist");

    let DefKind::Trait { methods } = &writer_trait.kind else {
        panic!("Writer should be a trait");
    };

    let method = context
        .get_def(methods[0])
        .expect("Writer::write definition should exist");

    let DefKind::Function { params, .. } = &method.kind else {
        panic!("Writer::write should be a function definition");
    };

    assert_eq!(
        params,
        &vec![
            Type::REF(Box::new(
                Type::GENERIC("Self".to_string())
            )),
            Type::U8,
        ],
    );
}

#[test]
fn trait_implementation_is_registered() {
    let context = analyze(
        r#"
trait Display {
    fn fmt(&self) -> void;
}

struct Thing {
}

extension Display on Thing {
    fn fmt(&self) -> void {
    }
}

fn main() -> void {
}
"#,
    );

    let display_def_id = find_definition(&context, "Display");
    let thing_def_id = find_definition(&context, "Thing");

    let trait_impl = context
        .trait_impls
        .iter()
        .find(|impl_| {
            impl_.trait_def_id == display_def_id &&
            matches!(
                &impl_.target_type,
                Type::STRUCT(type_ref) if type_ref.def_id == thing_def_id
            )
        })
        .expect("Display should be implemented for Thing");

    let fmt_def_id = *trait_impl
        .methods
        .get("fmt")
        .expect("Display implementation should contain fmt");

    let fmt = context
        .get_def(fmt_def_id)
        .expect("trait implementation method should exist");

    assert_eq!(
        fmt.absolute_path.join("::"),
        "Thing::<Display>::fmt",
    );
}

#[test]
fn trait_self_return_type_is_substituted_with_implementing_type() {
    analyze(
        r#"
trait Factory {
    fn make(&self) -> Self;
}

struct Thing {
}

extension Factory on Thing {
    fn make(&self) -> Thing {
        return Thing {};
    }
}

fn main() -> void {
}
"#,
    );
}

#[test]
fn trait_implementation_must_provide_required_methods() {
    assert_has_error(
        r#"
trait Display {
    fn fmt(&self) -> void;
}

struct Thing {
}

extension Display on Thing {
}

fn main() -> void {
}
"#,
        "S021",
    );
}

#[test]
fn trait_implementation_method_signature_must_match_contract() {
    assert_has_error(
        r#"
trait Writer {
    fn write(&mut self, value: u8) -> void;
}

struct Thing {
}

extension Writer on Thing {
    fn write(&self, value: u8) -> void {
    }
}

fn main() -> void {
}
"#,
        "S022",
    );
}

#[test]
fn trait_implementation_method_parameter_types_must_match_contract() {
    assert_has_error(
        r#"
trait Writer {
    fn write(&mut self, value: u8) -> void;
}

struct Thing {
}

extension Writer on Thing {
    fn write(&mut self, value: u16) -> void {
    }
}

fn main() -> void {
}
"#,
        "S022",
    );
}

#[test]
fn duplicate_trait_implementations_are_rejected() {
    assert_has_error(
        r#"
trait Display {
    fn fmt(&self) -> void;
}

struct Thing {
}

extension Display on Thing {
    fn fmt(&self) -> void {
    }
}

extension Display on Thing {
    fn fmt(&self) -> void {
    }
}

fn main() -> void {
}
"#,
        "S023",
    );
}

#[test]
fn extension_trait_target_must_actually_be_a_trait() {
    assert_has_error(
        r#"
struct NotATrait {
}

struct Thing {
}

extension NotATrait on Thing {
}

fn main() -> void {
}
"#,
        "S020",
    );
}

#[test]
fn trait_method_can_be_called_with_receiver_syntax() {
    analyze(
        r#"
trait Speak {
    fn speak(&self) -> void;
}

struct Thing {
}

extension Speak on Thing {
    fn speak(&self) -> void {
    }
}

fn main() -> void {
    let thing = Thing {};
    thing::speak();
}
"#,
    );
}

#[test]
fn inherent_method_takes_precedence_over_trait_method() {
    analyze(
        r#"
trait Describe {
    fn kind(&self) -> u16;
}

struct Thing {
}

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
"#,
    );
}

#[test]
fn ambiguous_trait_methods_are_rejected() {
    assert_has_error(
        r#"
trait First {
    fn run(&self) -> void;
}

trait Second {
    fn run(&self) -> void;
}

struct Thing {
}

extension First on Thing {
    fn run(&self) -> void {
    }
}

extension Second on Thing {
    fn run(&self) -> void {
    }
}

fn main() -> void {
    let thing = Thing {};
    thing::run();
}
"#,
        "S024",
    );
}

#[test]
fn generic_trait_implementation_matches_concrete_generic_type() {
    analyze(
        r#"
trait Touch {
    fn touch(&self) -> void;
}

struct Wrapper<T> {
    value: T;
}

extension<T> Touch on Wrapper<T> {
    fn touch(&self) -> void {
    }
}

fn touch_wrapper(wrapper: Wrapper<i32>) -> void {
    wrapper::touch();
}

fn main() -> void {
}
"#,
    );
}
