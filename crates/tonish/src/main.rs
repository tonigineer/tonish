use clap::Parser;
use std::process::ExitCode;
use tonish::args::{Args, load};

fn main() -> ExitCode {
    let args = Args::parse();
    match load(args).and_then(|shell| engine::run(Box::new(shell))) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("tonish: {}", e);
            ExitCode::FAILURE
        }
    }
}
