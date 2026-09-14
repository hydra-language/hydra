use std::{collections::HashMap, path::Path};
use std::fs;
use std::path::PathBuf;
use errors::error::{HydraError, Span};
use lexer::Lexer;

use crate::{Item, Type};
use crate::ast::NodeID;
use crate::parser::Parser;

pub struct ModuleTree {
    pub root: ModuleNode,
    pub worklist: Vec<UnresolvedUse>,

    // only files that are actually part of the compilation
    pub parsed_files: HashMap<PathBuf, (Vec<String>, Vec<Item>)>,

    // all module paths are resolved relative to the directory
    // containing the root source file passed to 'hydrac'
    project_root: PathBuf,
    stdlib_root: PathBuf,
}


pub struct ModuleNode {
    pub name: String,
    pub path: Vec<String>,
    pub file: Option<PathBuf>,
    pub children: HashMap<String, ModuleNode>,
    pub items: HashMap<String, ItemHeader>,
}

#[derive(Clone)]
pub struct ItemHeader {
    pub name: String,
    pub kind: ItemKind,
    pub is_pub: bool,
    pub id: NodeID,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ItemKind {
    FUNCTION,
    STRUCT,
    TRAIT,
    EXTENSION,
}

pub struct UnresolvedUse {
    pub in_module: Vec<String>,
    pub path: Vec<String>,
    pub alias: Option<String>,
    pub is_reexport: bool,
    pub state: ResolvedState,
}

pub enum ResolvedState {
    UNRESOLVED,
    RESOLVED(NodeID),
    ERROR(String),
}

enum ResolveStatus {
    DefinitelyMissing(String),
    BlockedOn(Vec<String>),
}

pub struct SourceMap {
    files: HashMap<PathBuf, String>,
    source_ids: HashMap<PathBuf, u32>,
    next_source_id: u32,
}

impl ModuleTree {

    pub fn build(entry: &Path, stdlib_root: PathBuf, source_map: &mut SourceMap)
        -> Result<Self, Vec<HydraError>>
    {
        let entry_file = entry.canonicalize().map_err(|e| {
            vec![HydraError::new(
                "C001",
                format!("could not resolve entry file '{}': {}", entry.display(), e),
                Span::default()
            )]
        })?;

        if !entry_file.is_file() {
            return Err(vec![HydraError::new(
                "C001",
                format!(
                    "compiler entry point must be a source file, found `{}`",
                    entry_file.display()
                ),
                Span::default(),
            )]);
        }

        let project_root = entry_file.parent()
            .unwrap_or(Path::new("."))
            .to_path_buf();

        let root_name = entry_file.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("<root>")
            .to_string();

        let mut tree = Self {
            root: ModuleNode {
                name: root_name,
                path: vec![],
                file: None,
                children: HashMap::new(),
                items: HashMap::new(),
            },
            worklist: Vec::new(),
            parsed_files: HashMap::new(),
            project_root,
            stdlib_root,
        };

        // the source file passed to 'hydrac <file>' is the only
        // initial compilation unit
        tree.load_module_file(&entry_file, vec![], source_map)?;

        Ok(tree)
    }

    fn module_node_mut(&mut self, module_path: &[String]) -> &mut ModuleNode {
        let mut current = &mut self.root;
        let mut accumulated_path = Vec::new();

        for segment in module_path {
            accumulated_path.push(segment.clone());

            current = current.children.entry(segment.clone()).or_insert_with(|| ModuleNode {
                name: segment.clone(),
                path: accumulated_path.clone(),
                file: None,
                children: HashMap::new(),
                items: HashMap::new(),
            });
        }

        current
    }

    pub fn resolve_imports(&mut self, source_map: &mut SourceMap) -> Result<(), Vec<HydraError>> {
        let mut progress = true;

        while progress {
            progress = false;

            // load_module_file() may append new imports to the worklist,
            // so only process the imports that existed at the beginning
            // of this iteration.
            let work_len = self.worklist.len();

            for i in 0..work_len {
                if !matches!(self.worklist[i].state, ResolvedState::UNRESOLVED) {
                    continue;
                }

                let path = self.worklist[i].path.clone();

                // it might already be resolvable from modules that have
                // previously been loaded.
                if let Ok(node_id) = self.try_resolve_path(&path) {
                    self.finish_import(i, node_id)?;

                    progress = true;
                    continue;
                }

                // not currently resolvable: try to pull the corresponding
                // module source file into the compilation.
                let loaded = self.ensure_import_loaded(&path, source_map)?;

                if loaded {
                    progress = true;
                }

                // loading the file populates ModuleNode::children/items,
                // so retry the resolution immediately.
                if let Ok(node_id) = self.try_resolve_path(&path) {
                    self.finish_import(i, node_id)?;

                    progress = true;
                }
            }
        }

        // the dependency graph has reached a fixpoint.
        // anything unresolved at this point is a real error.
        let mut errors = Vec::new();

        for i in 0..self.worklist.len() {
            if !matches!(self.worklist[i].state, ResolvedState::UNRESOLVED) {
                continue;
            }

            let path = self.worklist[i].path.clone();

            match self.try_resolve_path(&path) {
                Ok(node_id) => {
                    self.finish_import(i, node_id)?;
                }

                Err(ResolveStatus::DefinitelyMissing(msg)) => {
                    self.worklist[i].state = ResolvedState::ERROR(msg.clone());

                    errors.push(HydraError::new(
                        "M002",
                        format!("import error: {}", msg),
                        Span::default(),
                    ));
                }

                Err(ResolveStatus::BlockedOn(module)) => {
                    errors.push(HydraError::new(
                        "M001",
                        format!(
                            "unresolved import `{}` while resolving module `{}`",
                            path.join("::"),
                            module.join("::"),
                        ),
                        Span::default(),
                    ));
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn try_resolve_path(&self, path: &[String]) -> Result<NodeID, ResolveStatus> {
        if path.is_empty() {
            return Err(ResolveStatus::DefinitelyMissing("empty path".to_string()));
        }

        // start at the root of the project
        let mut current_node = &self.root;
        let start_idx = 0;

        // walk through the module/directory directories
        for segment in &path[start_idx..path.len() - 1] {
            match current_node.children.get(segment) {
                Some(child) => current_node = child,
                None => {
                    let avail: Vec<_> = current_node.children.keys().cloned().collect();
                    return Err(ResolveStatus::DefinitelyMissing(format!(
                        "module `{}` not found in `{}` (available: {:?})", 
                        segment, current_node.name, avail
                    )));
                }
            }
        }

        // the last segment could be a struct, fn, or a whole submodule
        let last_segment = &path[path.len() - 1];

        // 1. is it a direct item in the file? (Function, Struct, Trait)
        if let Some(item) = current_node.items.get(last_segment) {
            // visibility check (Phase 4) happens here!
            if !item.is_pub {
                return Err(ResolveStatus::DefinitelyMissing(format!("item `{}` is private", last_segment)));
            }
            return Ok(item.id);
        }

        // 2. is it a whole submodule? (e.g., importing a folder)
        if current_node.children.contains_key(last_segment) {
            return Ok(NodeID(0)); 
        }

        // 3. is it an unresolved re-export?
        // if `current_node` has `pub include` statements that are UNRESOLVED, 
        // the item *might* exist, we just don't know yet.
        let has_pending_reexports = self.worklist.iter().any(|use_| {
            use_.in_module == current_node.path &&
            use_.is_reexport && matches!(use_.state, ResolvedState::UNRESOLVED)
        });

        if has_pending_reexports {
            return Err(ResolveStatus::BlockedOn(current_node.path.clone()));
        }

        let avail_items: Vec<_> = current_node.items.keys().cloned().collect();
        Err(ResolveStatus::DefinitelyMissing(format!(
            "item `{}` not found in `{}` (available items: {:?})", 
            last_segment, current_node.name, avail_items
        )))
    }

    fn parse_file(file: &Path, current_module: &[String], source_map: &mut SourceMap) 
        -> Result<(Vec<Item>, HashMap<String, ItemHeader>, Vec<UnresolvedUse>), Vec<HydraError>> 
    {
        let source = match source_map.load_file(file) {
            Ok(source) => source.to_string(),

            Err(error) => {
                return Err(vec![error]);
            }
        };

        let source_id = source_map
            .get_source_id(file)
            .expect("ICE: loaded source has no source ID");

        let mut lexer = Lexer::new(source.clone());

        let tokens = match lexer.tokenize() {
            Ok(tokens) => tokens,

            Err(mut error) => {
                error.filepath = Some(file.display().to_string());
                error.source_text = Some(source);
                return Err(vec![error]);
            }
        };

        let mut parser = Parser::new(tokens, source_id);

        // we deliberately parse the entire demanded file.
        parser.headers_only = false;

        let ast = match parser.parse() {
            Ok(ast) => ast,

            Err(mut errors) => {
                for error in &mut errors {
                    error.filepath = Some(file.display().to_string());
                    error.source_text = Some(source.clone());
                }

                return Err(errors);
            }
        };

        let reexport_includes = file.file_name().and_then(|name| name.to_str()) == Some("mod.hydra");

        let mut headers = HashMap::new();
        let mut imports = Vec::new();

        for item in &ast {
            match item {
                Item::Function(decl) => {
                    headers.insert(
                        decl.name.lexeme.clone(),
                        ItemHeader {
                            name: decl.name.lexeme.clone(),
                            kind: ItemKind::FUNCTION,
                            is_pub: decl.is_pub,
                            id: decl.id,
                        },
                    );
                }

                Item::Struct(decl) => {
                    headers.insert(
                        decl.name.lexeme.clone(),
                        ItemHeader {
                            name: decl.name.lexeme.clone(),
                            kind: ItemKind::STRUCT,
                            is_pub: decl.is_pub,
                            id: decl.id,
                        },
                    );
                }

                Item::Trait(decl) => {
                    headers.insert(
                        decl.name.lexeme.clone(),
                        ItemHeader {
                            name: decl.name.lexeme.clone(),
                            kind: ItemKind::TRAIT,
                            is_pub: decl.is_pub,
                            id: decl.id,
                        },
                    );
                }

                Item::Include(decl) => {
                    if let Type::Path { segments, .. } = &decl.path {
                        let base_path: Vec<String> = segments.iter().map(|s| s.lexeme.clone()).collect();

                        //
                        // include foo::bar::{A, B};
                        //
                        // represent each imported item individually in the dependency
                        // graph
                        //
                        // this also means a mod.hydra can re-export selective
                        // imports correctly
                        //
                        if let Some(symbols) = &decl.symbols {
                            for symbol in symbols {
                                let mut path = base_path.clone();
                                path.push(symbol.lexeme.clone());

                                imports.push(UnresolvedUse {
                                    in_module: current_module.to_vec(),
                                    path,
                                    alias: None,
                                    is_reexport: reexport_includes,
                                    state: ResolvedState::UNRESOLVED
                                });
                            }
                        } else {
                            imports.push(UnresolvedUse {
                                in_module: current_module.to_vec(),
                                path: base_path,
                                alias: decl.alias.as_ref().map(|t| t.lexeme.clone()),
                                is_reexport: reexport_includes,
                                state: ResolvedState::UNRESOLVED
                            })
                        }
                    }
                }

                Item::Extension(_) => {
                    // Extensions do not introduce a module-level name.
                }
            }
        }

        Ok((ast, headers, imports))
    }

    pub fn parse_bodies(&mut self, _source_map: &SourceMap) -> Result<(), Vec<HydraError>> {
        // for backwards compatibility, eventually will get rid of
        // files are fully parsed when they enter the dependency graph.
        Ok(())
    }

    fn load_module_file(&mut self, file: &Path, module_path: Vec<String>, source_map: &mut SourceMap) 
        -> Result<bool, Vec<HydraError>> 
    {
        let file = file.canonicalize().map_err(|e| {
            vec![HydraError::new(
                "C001",
                format!(
                    "could not resolve module file `{}`: {}",
                    file.display(),
                    e
                ),
                Span::default(),
            )]
        })?;

        // Already part of the compilation.
        if self.parsed_files.contains_key(&file) {
            return Ok(false);
        }

        let (ast, headers, imports) = ModuleTree::parse_file(&file, &module_path, source_map)?;

        {
            let node = self.module_node_mut(&module_path);

            node.file = Some(file.clone());
            node.items = headers;
        }

        self.worklist.extend(imports);
        self.parsed_files.insert(file, (module_path, ast));

        Ok(true)
    }

    fn ensure_import_loaded(&mut self, path: &[String], source_map: &mut SourceMap) 
        -> Result<bool, Vec<HydraError>> 
    {
        if path.is_empty() {
            return Ok(false);
        }

        // decide which physical source tree owns this logical path.
        let source_root = self.source_root_for(path).to_path_buf();

        //
        // an import may refer to either:
        //
        //     foo/bar.hydra
        //
        // or:
        //
        //     foo/bar/mod.hydra
        //
        // both represent the logical module `foo::bar`
        //
        // as before, search for the deepest module prefix
        //
        for prefix_len in (1..=path.len()).rev() {
            let module_path = &path[..prefix_len];
            let mut module_stem = source_root.clone();

            for segment in module_path {
                module_stem.push(segment);
            }

            let mut file_candidate = module_stem.clone();
            file_candidate.set_extension("hydra");

            let directory_candidate = module_stem.join("mod.hydra");

            let has_file = file_candidate.is_file();
            let has_directory_module = directory_candidate.is_file();

            if has_file && has_directory_module {
                return Err(vec![HydraError::new(
                    "M003",
                    format!(
                        "module `{}` is defined by both `{}` and `{}`",
                        module_path.join("::"),
                        file_candidate.display(),
                        directory_candidate.display(),
                    ),
                    Span::default(),
                )]);
            }

            let candidate = if has_file {
                Some(file_candidate)
            } else if has_directory_module {
                Some(directory_candidate)
            } else {
                None
            };

            let Some(candidate) = candidate else { continue; };

            let loaded = self.load_module_file(&candidate, module_path.to_vec(), source_map)?;
            if loaded {
                return Ok(true)
            }
        }

        Ok(false)
    }

    pub fn is_module(&self, path: &[String]) -> bool {
        let mut current_node = &self.root;
        for segment in path {
            if let Some(child) = current_node.children.get(segment) {
                current_node = child;
            } else {
                return false;
            }
        }
        true
    }

    fn source_root_for(&self, path: &[String]) -> &Path {
        match path.first().map(String::as_str) {
            Some("core" | "alloc" | "std") => {
                &self.stdlib_root
            }

            _ => {
                &self.project_root
            }
        }
    }

    fn item_header(&self, path: &[String]) -> Option<&ItemHeader> {
        if path.is_empty() {
            return None;
        }

        let mut current = &self.root;

        for segment in &path[..path.len() - 1] {
            current = current.children.get(segment)?;
        }

        current.items.get(path.last()?)
    }

    fn finish_import(&mut self, index: usize, node_id: NodeID) -> Result<(), Vec<HydraError>> {
        let is_reexport = self.worklist[index].is_reexport;
        let target_path = self.worklist[index].path.clone();

        let in_module = self.worklist[index].in_module.clone();
        let export_name = self.worklist[index].alias.clone().or_else(|| target_path.last().cloned());

        if is_reexport {
            if let (Some(export_name), Some(mut header)) = (export_name, self.item_header(&target_path).cloned()) 
            {
                header.name = export_name.clone();

                //
                // the item is visible through the module facade even though
                // its canonical definition path remains unchanged.
                //
                header.is_pub = true;
                let module = self.module_node_mut(&in_module);

                if let Some(existing) = module.items.get(&export_name) {
                    if existing.id != header.id {
                        return Err(vec![HydraError::new(
                            "M003",
                            format!(
                                "module `{}` already defines `{}`",
                                in_module.join("::"),
                                export_name,
                            ),
                            Span::default(),
                        )]);
                    }
                } else {
                    module.items.insert(export_name, header);
                }
            }
        }

        self.worklist[index].state = ResolvedState::RESOLVED(node_id);

        Ok(())
    }
}

impl SourceMap {

    pub fn new() -> Self {
        Self {
            files: HashMap::new(),
            source_ids: HashMap::new(),
            next_source_id: 1,
        }
    }

    pub fn load_file(&mut self, path: &Path) -> Result<&str, HydraError> {
        if !self.files.contains_key(path) {
            let source = fs::read_to_string(path).map_err(|e| {
                HydraError::new(
                    "C001",
                    format!("could not read file: {}", e),
                    Span::default(),
                )
            })?;

            let source_id = self.next_source_id;
            self.next_source_id += 1;

            self.files.insert(
                path.to_path_buf(),
                source,
            );

            self.source_ids.insert(
                path.to_path_buf(),
                source_id,
            );
        }

        Ok(self.files.get(path).unwrap().as_str())
    }

    pub fn get_source_id(&self, path: &Path) -> Option<u32> {
        self.source_ids.get(path).copied()
    }

    pub fn get_source(&self, path: &Path) -> Option<&str> {
        self.files.get(path).map(|s| s.as_str())
    }
}
