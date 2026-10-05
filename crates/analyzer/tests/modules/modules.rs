use std::{
    fs,
    path::{
        Path,
        PathBuf,
    },
    sync::atomic::{
        AtomicUsize,
        Ordering,
    },
};

use analyzer::{
    Analyzer,
    Resolver,
};

use ir::{
    context::HIRContext,
    types::Type,
};

use parser::module::{
    ModuleTree,
    SourceMap,
};


static NEXT_TEST_ID: AtomicUsize =
    AtomicUsize::new(0);


// ============================================================================
// TEST PROJECT
// ============================================================================

struct TestProject {
    dir: PathBuf,
    entry: PathBuf,
}


impl TestProject {

    fn new(
        entry_source: &str,
    ) -> Self {
        let id =
            NEXT_TEST_ID.fetch_add(
                1,
                Ordering::Relaxed,
            );

        let dir =
            std::env::temp_dir().join(
                format!(
                    "hydrac-analyzer-module-reexport-test-{}-{}",
                    std::process::id(),
                    id,
                )
            );

        fs::create_dir_all(
            &dir,
        )
        .expect(
            "failed to create temporary test directory"
        );

        let entry =
            dir.join(
                "main.hydra"
            );

        fs::write(
            &entry,
            entry_source,
        )
        .expect(
            "failed to write entry source"
        );

        Self {
            dir,
            entry,
        }
    }


    fn write(
        &self,
        relative: &str,
        source: &str,
    ) {
        let path =
            self.dir.join(
                relative
            );

        if let Some(parent) =
            path.parent()
        {
            fs::create_dir_all(
                parent
            )
            .expect(
                "failed to create module directory"
            );
        }

        fs::write(
            path,
            source,
        )
        .expect(
            "failed to write module source"
        );
    }
}


impl Drop for TestProject {

    fn drop(&mut self) {
        let _ =
            fs::remove_dir_all(
                &self.dir
            );
    }
}


fn path(
    segments: &[&str],
) -> Vec<String> {
    segments
        .iter()
        .map(|segment| {
            (*segment).to_string()
        })
        .collect()
}


// ============================================================================
// MODULE RE-EXPORTS
// ============================================================================

#[test]
fn mod_hydra_reexport_resolves_to_original_def_id() {
    let project =
        TestProject::new(
            r#"
include core::ptr::NonNull;

fn consume(
    value: NonNull
) -> void {
}

fn main() -> void {
}
"#,
        );


    project.write(
        "core/ptr/mod.hydra",
        r#"
include core::ptr::non_null::NonNull;
"#,
    );


    project.write(
        "core/ptr/non_null.hydra",
        r#"
pub struct NonNull {
    address: usize;
}
"#,
    );


    let mut source_map =
        SourceMap::new();


    let mut module_tree =
        ModuleTree::build(
            Path::new(
                &project.entry
            ),
            project.dir.clone(),
            &mut source_map,
        )
        .expect(
            "module tree should build"
        );


    module_tree
        .resolve_imports(
            &mut source_map
        )
        .expect(
            "directory module re-export should resolve"
        );


    let mut context =
        HIRContext::default();


    let resolver =
        Resolver::new(
            &module_tree,
            &mut context,
            &source_map,
        );


    let (
        name_resolver,
        global_symbols,
    ) =
        resolver
            .resolve()
            .expect(
                "name resolution should accept the re-exported path"
            );


    let original_path =
        path(
            &[
                "core",
                "ptr",
                "non_null",
                "NonNull",
            ]
        );

    let exported_path =
        path(
            &[
                "core",
                "ptr",
                "NonNull",
            ]
        );


    let original_def =
        *global_symbols
            .get(
                &original_path
            )
            .expect(
                "original NonNull definition should exist"
            );

    let exported_def =
        *global_symbols
            .get(
                &exported_path
            )
            .expect(
                "core::ptr::NonNull re-export should exist"
            );


    assert_eq!(
        exported_def,
        original_def,
        "a re-export must be an alternate path to the original DefID",
    );


    let hir =
        Analyzer::new(
            &module_tree,
            &mut context,
            &source_map,
            name_resolver,
            global_symbols,
        )
        .analyze()
        .expect(
            "program using re-exported NonNull should analyze"
        );


    let consume =
        hir.functions
            .iter()
            .find(|function| {
                function.name
                    == "consume"
            })
            .expect(
                "consume function should exist"
            );


    let (
        _,
        parameter_ty,
    ) =
        consume.params
            .first()
            .expect(
                "consume should have one parameter"
            );


    let Type::STRUCT(type_ref) =
        parameter_ty
    else {
        panic!(
            "expected imported NonNull parameter to resolve to struct, found {parameter_ty:#?}"
        );
    };


    assert_eq!(
        type_ref.def_id,
        original_def,
        "use of core::ptr::NonNull must preserve the original nominal type identity",
    );

    assert_eq!(
        type_ref.symbol,
        "core::ptr::non_null::NonNull",
        "the canonical definition path should remain the path where NonNull is defined",
    );
}
