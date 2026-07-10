use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::fs;

use crate::ast::*;
use crate::parser::Parser;
use errors::error::{HydraError, Span};
use lexer::Lexer;

pub struct Program<'a> {
    pub modules: HashMap<Vec<String>, (&'a str, Vec<Item<'a>>)>
}

impl<'a> Program<'a> {

    pub fn new() -> Self {
        Self {
            modules: HashMap::new()
        }
    }

    pub fn build(entry: &Path) -> Result<Self, HydraError> {
        let mut program = Program::new();
        let root = entry.file_stem().unwrap().to_str().unwrap().to_string();

        program.load_module_recursive(entry, vec![root])?;

        Ok(program)
    }

    fn load_module_recursive(&mut self, file: &Path, module_path: Vec<String>) -> Result<(), HydraError> {
        if self.modules.contains_key(&module_path) {
            return Ok(());
        }

        let filename = file.to_str().unwrap_or("unknown");

        let source = match fs::read_to_string(file) {
            Ok(s) => s,
            Err(err) => {
                let e = HydraError::new("C001", format!("could not read file {}: {}", file.display(), err), Span::default())
                    .with_help("check if the file path is correct and accessible");
                
                e.report("", filename);
                return Err(HydraError::new("C000", "compilation aborted due to file system error.", Span::default()));
            }
        };

        let leaked_source: &'a str = Box::leak(source.into_boxed_str());

        let mut lexer = Lexer::new(leaked_source);
        let tokens = match lexer.tokenize() {
            Ok(t) => t,
            Err(e) => {
                e.report(leaked_source, filename);
                return Err(HydraError::new("C002", format!("lexical analysis failed in {}", file.display()), Span::default()));
            }
        };

        let mut parser = Parser::new(tokens);
        let ast = match parser.parse() {
            Ok(a) => {
                a
            }
            Err(errors) => {
                for e in errors {
                    e.report(leaked_source, filename);
                }
                return Err(HydraError::new("C003", format!("syntax analysis failed in {}", file.display()), Span::default()));
            }
        };

        let mut sub_modules: Vec<(Vec<String>, Span)> = Vec::new();
        
        // scan for include statements instead of the removed module declaration.
        for node in &ast {
            if let Item::Include(include_decl) = node {
                if let Type::Path { segments, .. } = &include_decl.path {
                    let strings: Vec<String> = segments.iter().map(|s| s.lexeme.to_string()).collect();
                    if !strings.is_empty() {
                        sub_modules.push((strings, segments[0].span));
                    }
                }
            }
        }

        self.modules.insert(module_path.clone(), (leaked_source, ast));

        let current_dir = file.parent().unwrap_or(Path::new(""));

        for (sub_mod_path, span) in sub_modules {
            let mut next = module_path.clone();
            next.extend(sub_mod_path.clone());

            let mut rel_path = PathBuf::new();
            for seg in &sub_mod_path {
                rel_path.push(seg);
            }

            let first_seg = &sub_mod_path[0];
            let resolved_path = if first_seg == "std" || first_seg == "core" || first_seg == "alloc" {
                let home = std::env::var("HOME").unwrap_or_else(|_| String::from("~"));
                let mut lib_path = PathBuf::from(home);

                lib_path.push(".hydra");
                lib_path.push(&rel_path);
                lib_path.push("lib.hydra");

                lib_path
            } else {
                let single_file = current_dir.join(rel_path.with_extension("hydra"));
                let folder_file = current_dir.join(&rel_path).join("lib.hydra");

                if single_file.exists() {
                    single_file
                } else if folder_file.exists() {
                    folder_file
                } else {
                    let e = HydraError::new(
                        "C004", 
                        format!("module '{}' not found", sub_mod_path.join("::")), 
                        span
                    ).with_help(format!("looked for {} and {}", single_file.display(), folder_file.display()));
                    
                    e.report(leaked_source, filename);
                    return Err(HydraError::new("C005", "compilation aborted due to missing modules.", Span::default()));
                }
            };

            self.load_module_recursive(&resolved_path, next)?;
        }

        Ok(())
    }
}
