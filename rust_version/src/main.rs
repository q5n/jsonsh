use std::io::{self, IsTerminal};

use jsonsh::cli::{self, Input};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let stdin = io::stdin();
    let terminal = stdin.is_terminal();
    let input = Input {
        reader: stdin,
        terminal,
    };
    let mut stdout = io::stdout();
    match cli::run(&args, input, &mut stdout) {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            eprintln!("jsonsh: {}", e);
            std::process::exit(1);
        }
    }
}
