//! toniSH : A wayland desktop shell
//!
//!     toniSH --version

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("-v" | "--version") => {
            println!("toniSH {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        _ => {
            println!("usage: toniSH [--version]");
            ExitCode::from(2)
        }
    }
    // code.exit_process()  <- unstable feature, use when available
}
