use clap::Parser;
use std::process::ExitCode;
use tonish::args::{Args, Command, load};
use tonish::ipc;

fn main() -> ExitCode {
    let args = Args::parse();
    let done = match &args.command {
        Some(Command::Msg { message }) => ipc::socket_from_env()
            .ok_or_else(|| "XDG_RUNTIME_DIR is not set, so there is no socket to send to".into())
            .and_then(|socket| ipc::send(&socket, *message)),
        None => load(args).and_then(|shell| engine::run(Box::new(shell))),
    };
    match done {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("tonish: {}", e);
            ExitCode::FAILURE
        }
    }
}
