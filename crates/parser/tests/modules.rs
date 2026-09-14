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
                    "hydrac-parser-module-test-{}-{}",
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


// ============================================================================
// DIRECTORY MODULES
// ============================================================================

#[test]
fn mod_hydra_reexports_included_item_from_directory_module() {
    let project =
        TestProject::new(
            r#"
include core::ptr::NonNull;
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

    let mut tree =
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


    tree.resolve_imports(
        &mut source_map
    )
    .expect(
        "core::ptr::NonNull should resolve through core/ptr/mod.hydra"
    );


    let core =
        tree.root
            .children
            .get("core")
            .expect(
                "core module should be loaded"
            );

    let ptr =
        core.children
            .get("ptr")
            .expect(
                "core::ptr module should be loaded"
            );

    let non_null =
        ptr.children
            .get("non_null")
            .expect(
                "core::ptr::non_null module should be loaded"
            );


    let original =
        non_null.items
            .get("NonNull")
            .expect(
                "NonNull should exist in its defining module"
            );

    let exported =
        ptr.items
            .get("NonNull")
            .expect(
                "NonNull should be re-exported by core::ptr"
            );


    assert_eq!(
        exported.id,
        original.id,
        "re-export must reference the original definition",
    );

    assert!(
        exported.is_pub,
        "a mod.hydra include must be visible through the directory module",
    );
}


#[test]
fn ordinary_hydra_file_still_defines_module() {
    let project =
        TestProject::new(
            r#"
include core::ptr::NonNull;
"#,
        );

    project.write(
        "core/ptr.hydra",
        r#"
pub struct NonNull {
    address: usize;
}
"#,
    );


    let mut source_map =
        SourceMap::new();

    let mut tree =
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


    tree.resolve_imports(
        &mut source_map
    )
    .expect(
        "existing file modules must continue to resolve"
    );


    let ptr =
        tree.root
            .children
            .get("core")
            .and_then(|core| {
                core.children.get("ptr")
            })
            .expect(
                "core::ptr should exist"
            );

    assert!(
        ptr.items.contains_key(
            "NonNull"
        ),
        "ptr.hydra should continue to define core::ptr",
    );
}


#[test]
fn file_module_and_directory_module_cannot_define_same_module() {
    let project =
        TestProject::new(
            r#"
include core::ptr::NonNull;
"#,
        );

    project.write(
        "core/ptr.hydra",
        r#"
pub struct NonNull {
    address: usize;
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

    let mut tree =
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


    let errors =
        tree.resolve_imports(
            &mut source_map
        )
        .expect_err(
            "two physical files must not define the same logical module"
        );


    assert!(
        errors.iter().any(|error| {
            error.code == "M003" &&
            error.message.contains(
                "defined by both"
            )
        }),
        "expected duplicate module definition diagnostic, found {errors:#?}",
    );
}
