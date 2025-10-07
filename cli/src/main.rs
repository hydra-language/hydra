use std::{fs, path::Path, process::{self, Command}};

use clap::{Arg, Command as ClapCommand};
use inkwell::context::Context;
use lexer::Lexer;
use parser::parser::Parser;
use parser::type_check::TypeChecker;
use codegen::CodeGen;

fn main() {
    let matches = ClapCommand::new("hydrac")
        .version(env!("CARGO_PKG_VERSION"))
        .about("Hydra Compiler CLI")
        .arg(
            Arg::new("input")
                .help("Input .hydra source file")
                .required(false)
                .value_name("INPUT")
                .index(1),
        )
        .arg(
            Arg::new("tokens")
                .long("tokens")
                .help("Emit tokens to a .tokens file")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("ast")
                .long("ast")
                .help("Emit AST nodes to a .nodes file")
                .action(clap::ArgAction::SetTrue)
        )
        .arg(
            Arg::new("ir")
                .long("ir")
                .help("Emir llvm ir to a .ir file")
                .action(clap::ArgAction::SetTrue)
        )
        .get_matches();

    let input = match matches.get_one::<String>("input") {
        Some(i) => i,
        None => {
            ClapCommand::new("hydrac")
                .version(env!("CARGO_PKG_VERSION"))
                .about("Hydra Compiler CLI")
                .arg(
                    Arg::new("input")
                        .help("Input .hydra source file")
                        .required(false)
                        .value_name("INPUT")
                        .index(1),
                )
                .arg(
                    Arg::new("tokens")
                        .long("tokens")
                        .help("Emit tokens to a .tokens file")
                        .action(clap::ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("ast")
                        .long("ast")
                        .help("Emit AST nodes to a .nodes file")
                        .action(clap::ArgAction::SetTrue)
                )
                .arg(
                    Arg::new("ir")
                        .long("ir")
                        .help("Emir llvm ir to a .ir file")
                        .action(clap::ArgAction::SetTrue)
                )
                .print_help()
                .unwrap();
            println!();
            process::exit(0)
        }
    };

    let emit_tokens = matches.get_flag("tokens");
    let emit_ast = matches.get_flag("ast");
    let emit_ir = matches.get_flag("ir");
    let input_path = Path::new(input);

    // --- Get the file stem for naming the module and output file ---
    let module_name = input_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");

    // Validate .hydra extension
    match input_path.extension().and_then(|e| e.to_str()) {
        Some("hydra") => {}
        _ => {
            eprintln!("error: '{}' is not a .hydra file", input);
            process::exit(1);
        }
    }

    // Read source file
    let contents = match fs::read_to_string(input) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error reading '{}': {}", input, e);
            process::exit(1);
        }
    };

    // Run lexer
    let mut lexer = Lexer::new(&contents);
    let tokens = match lexer.tokenize() {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Lexer error: {}", e);
            process::exit(1);
        }
    };

    // Emit tokens to a file if requested
    if emit_tokens {
        let token_output = tokens
            .iter()
            .map(|t| format!("{:?}", t))
            .collect::<Vec<_>>()
            .join("\n");
        let token_filename = input_path.with_extension("tokens").to_string_lossy().into_owned();
        if let Err(e) = fs::write(&token_filename, token_output) {
            eprintln!("error writing token file '{}': {}", token_filename, e);
            process::exit(1);
        }
        println!("Tokens written to: {}", token_filename);
        return;
    }

    // Run parser
    let mut parser = Parser::new(tokens);
    let ast = match parser.parse() {
        Ok(ast) => ast,
        Err(e) => {
            eprintln!("Parser error: {}", e);
            process::exit(1);
        }
    };

    if emit_ast {
        let ast_output = ast
            .iter()
            .map(|node| format!("{:#?}", node))
            .collect::<Vec<_>>()
            .join("\n\n");
        let ast_filename = input_path.with_extension("nodes").to_string_lossy().into_owned();
        if let Err(e) = fs::write(&ast_filename, ast_output) {
            eprintln!("error: writing AST file '{}': {}", ast_filename, e);
            process::exit(1);
        }
        println!("AST written to: {}", ast_filename);
        return;
    }

    let mut type_checker = TypeChecker::new();
    if let Err(e) = type_checker.check(&ast) {
        eprintln!("error: {}", e);
        process::exit(1);
    };

    let context = Context::create();
    let mut codegen = CodeGen::new(&context, module_name);

    if let Err(e) = codegen.generate(&ast) {
        eprintln!("CodeGen error: {}", e);
        process::exit(1);
    }

    if emit_ir {
        let ir_output = codegen.ir_to_string();
        let ir_filename = input_path.with_extension("ir").to_string_lossy().into_owned();

        if let Err(e) = fs::write(&ir_filename, ir_output) {
            eprintln!("error: writing IR file '{}' failed: {}", ir_filename, e);
            process::exit(1);
        }
        println!("IR written to: {}", ir_filename);
        return;
    }

    // Compile to object file
    let obj_path_str = format!("{}.o", module_name);
    let obj_path = Path::new(&obj_path_str);
    if let Err(e) = codegen.write_to_object_file(obj_path) {
        eprintln!("error writing object file: {}", e);
        process::exit(1);
    }

    // Link the object file into an executable
    let linker_output = Command::new("cc")
        .arg(&obj_path_str)
        .arg("-o")
        .arg(module_name)
        .output()
        .expect("Failed to execute linker");

    if !linker_output.status.success() {
        eprintln!(
            "Linker error:\n{}",
            String::from_utf8_lossy(&linker_output.stderr)
        );
        process::exit(1);
    }

    // Clean up the temporary object file
    if let Err(e) = fs::remove_file(obj_path) {
        eprintln!("Warning: could not remove temporary object file: {}", e);
    }
}
