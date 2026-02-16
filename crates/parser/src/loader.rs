use std::collections::{HashMap, HashSet};
use std::fs;
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
