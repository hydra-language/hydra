use std::collections::{HashMap, HashSet};
use std::{env, fs};
use std::path::{PathBuf};
use lexer::Lexer;
use crate::parser::Parser;
use crate::{ASTNode, ParserError};

pub struct ExternalLoader<'a> {
    pub cache: HashMap<PathBuf, Vec<ASTNode<'a>>>,
    pub integrated: HashSet<String>,
    empty: Vec<ASTNode<'a>>,
}

impl<'a> ExternalLoader<'a> {

    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
            integrated: HashSet::new(),
            empty: Vec::new(),
        }
    }

    pub fn load(&mut self, module_path: &str) -> Result<&Vec<ASTNode<'a>>, String> {
        let mut actual_path = PathBuf::new();

        let home = env::var("HOME")
            .or_else(|_| env::var("USERPROFILE"))
            .map_err(|_| "could not find home directory to resolve std::".to_string()
            )?;

        let is_lib_path = module_path.starts_with("std::") || 
                          module_path.starts_with("core::") || 
                          module_path.starts_with("alloc::");

        if is_lib_path {
            actual_path.push(home);
            actual_path.push(".hydra");
            actual_path.push("std");

            let segments = module_path.split("::");
            for segment in segments {
                actual_path.push(segment);
            }
        } else {
            let segments = module_path.split("::");
            for segment in segments {
                actual_path.push(segment);
            }
        }

        actual_path.set_extension("hydra");

        let source = std::fs::read_to_string(&actual_path)
            .map_err(|_| format!("error: file not found at {:?}", actual_path))?;

        let path = PathBuf::from(format!("{}.hydra", module_path));
        let abs_path = fs::canonicalize(&path)
            .map_err(|_| format!("could not find module file: {:?}", path))?;

        // 1. Check Cache
        if self.cache.contains_key(&abs_path) {
            return Ok(self.cache.get(&abs_path).unwrap());
        }

        self.cache.insert(abs_path.clone(), Vec::new());

        // 2. Read File
        let source = fs::read_to_string(&abs_path)
            .map_err(|e| format!("failed to read {}: {}", module_path, e))?;
        
        // 3. Register Source (leaked to 'a to allow Parser to reference it)
        let leaked_source: &'a str = Box::leak(source.into_boxed_str());

        // 4. Lex & Parse
        let mut lexer = Lexer::new(leaked_source);
        let tokens = lexer.tokenize()?;
        
        let ast = {
            let mut parser = Parser::new(tokens, self);
            match parser.parse() {
                Ok(nodes) => nodes,
                Err(errs) => {
                    let fname = abs_path.to_string_lossy();

                    for err in &errs {
                        err.report(leaked_source, &fname);
                    }

                    return Err(format!("module '{}' contains errors", module_path));
                }
            }
        };

        // 5. Store and Return
        self.cache.insert(abs_path.clone(), ast);
        Ok(self.cache.get(&abs_path).unwrap())
    }
}
