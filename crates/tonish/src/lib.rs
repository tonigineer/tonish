//! The shell: what tonish shows, and how it is set up and controlled. It
//! implements the engine's [`App`](engine::App) and never touches Wayland
//! itself, so that all of it can be tested in plain processes; `main.rs`
//! only reads the command line and hands over.

pub mod args;
pub mod config;
pub mod ipc;
pub mod logging;
pub mod shell;
pub mod theme;
