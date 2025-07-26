use std::{fs, path::Path, process};

use clap::{Arg, Command};
use lexer::Lexer;

fn main() {
    let matches = Command::new("hydrac")
        .version(env!("CARGO_PKG_VERSION"))
        .about("Hydra Compiler CLI")
        .arg(
            Arg::new("input")
                .help("Input .hydra source file")
                .required(true)
                .value_name("INPUT")
                .index(1),
        )
        .arg(
            Arg::new("output")
                .short('o')
                .long("output")
                .value_name("FILE")
                .help("Specify output filename (default: <input>_output)"),
        )
        .arg(
            Arg::new("tokens")
                .long("tokens")
                .help("Emit tokens to a .tokens file")
                .action(clap::ArgAction::SetTrue),
        )
        .get_matches();

    let input = matches
        .get_one::<String>("input")
        .expect("Input file is required");
    let emit_tokens = matches.get_flag("tokens");
    let output_file = matches.get_one::<String>("output");

    let input_path = Path::new(input);

    // Validate .hydra extension
    match input_path.extension().and_then(|e| e.to_str()) {
        Some("hydra") => {}
        _ => {
            eprintln!("Error: '{}' is not a .hydra file", input);
            process::exit(1);
        }
    }

    // Read source file
    let contents = match fs::read_to_string(input) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error reading '{}': {}", input, e);
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

        let token_filename = input_path
            .with_extension("tokens")
            .to_string_lossy()
            .into_owned();

        if let Err(e) = fs::write(&token_filename, token_output) {
            eprintln!("Error writing token file '{}': {}", token_filename, e);
            process::exit(1);
        }

        println!("Tokens written to: {}", token_filename);
        return;
    }

    // Generate basic output
    let mut output = String::new();
    output.push_str("=== HYDRA COMPILER ===\n");
    output.push_str(&format!("Compiling file: {}\n\n", input));
    output.push_str("=== TOKENIZING ===\n");
    output.push_str("Tokenization successful\n");
    output.push_str(&format!("Tokens found: {}\n", tokens.len()));

    // Determine output file path
    let final_output = output_file
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            let stem = input_path.file_stem().unwrap().to_str().unwrap();
            format!("{}_output", stem)
        });

    // Write output file
    if let Err(e) = fs::write(&final_output, output) {
        eprintln!("Error writing output file '{}': {}", final_output, e);
        process::exit(1);
    }

    println!("Output written to: {}", final_output);
}
