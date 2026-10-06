mod args;

use std::process;

use args::Cli;
use clap::{
    CommandFactory,
    Parser,
};
use driver::Compiler;

fn main() {
    let cli = Cli::parse();

    let Ok(Some(options)) = cli.into_options() else {
        Cli::command().print_help().unwrap();
        println!();

        process::exit(1);
    };

    let result = Compiler::new(options)
        .and_then(|mut compiler| compiler.compile());

    if let Err(error) = result {
        error.report();
        process::exit(1);
    }
}
