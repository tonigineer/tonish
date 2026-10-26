//! toniSH : A wayland desktop shell
//!
//!     toniSH --version

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("-v" | "--version") => {
            println!("toniSH {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS
        }
        _ => {
            println!("usage: toniSH [--version]");
            return ExitCode::from(2);
        }
    };
}
