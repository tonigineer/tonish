//! toniSH : A wayland desktop shell
//!
//!     tonish
//!     tonish --version

use std::process::ExitCode;
use tonish::shell::Shell;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("-v" | "--version") => {
            println!("toniSH {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some("-h" | "--help") => ExitCode::SUCCESS,
        None => match engine::run(Box::new(Shell::default())) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("{}", e);
                ExitCode::FAILURE
            }
        },
        _ => {
            println!("usage: toniSH [--version]");
            ExitCode::from(2)
        }
    }
    // code.exit_process()  <- unstable feature, use when available
}
