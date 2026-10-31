//! A line of text each way, so that scripts can talk to tonish with `socat`
//! or `nc -U` as well as with `tonish msg`.

use std::ffi::OsString;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};

use clap::ValueEnum;

// clap turns the doc comments on the variants into `tonish msg --help`, so
// they are written for the people running tonish.

/// One enum for both ends, so that the names `tonish msg` sends and the
/// names the shell understands cannot drift apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Message {
    /// Close all panels and end tonish
    Quit,
}

impl Message {
    pub fn name(self) -> String {
        self.to_possible_value()
            .expect("no message is hidden from clap")
            .get_name()
            .to_owned()
    }

    pub fn parse(text: &str) -> Option<Self> {
        Self::from_str(text, false).ok()
    }
}

/// One socket per Wayland display, because a nested session or a second
/// seat runs a tonish of its own. `$XDG_RUNTIME_DIR` is private to the user
/// and cleared at logout, which is what a socket needs; without it there is
/// nowhere safe to put one. libwayland falls back to `wayland-0` when
/// `$WAYLAND_DISPLAY` is unset, and so does this. The variables are passed
/// in, as in `args::default_config`, so that tests need not change the
/// environment.
pub fn socket(runtime_dir: Option<OsString>, display: Option<OsString>) -> Option<PathBuf> {
    let dir = PathBuf::from(runtime_dir?);
    let display = display.map_or_else(|| PathBuf::from("wayland-0"), PathBuf::from);
    // `$WAYLAND_DISPLAY` may be a full path to the compositor's socket.
    let name = display.file_name()?.to_string_lossy().into_owned();
    Some(dir.join(format!("tonish-{name}.sock")))
}

pub fn socket_from_env() -> Option<PathBuf> {
    socket(
        std::env::var_os("XDG_RUNTIME_DIR"),
        std::env::var_os("WAYLAND_DISPLAY"),
    )
}

/// Reads to the end of the connection, not just the reply line, because the
/// shell keeps the connection of a `quit` open until it has shut down. So
/// `tonish msg quit && tonish` starts the new one after the old is gone.
pub fn send(socket: &Path, message: Message) -> Result<(), String> {
    let mut stream = UnixStream::connect(socket)
        .map_err(|e| format!("no tonish answers at {}: {e}", socket.display()))?;
    writeln!(stream, "{}", message.name()).map_err(|e| e.to_string())?;
    let mut reply = String::new();
    stream
        .read_to_string(&mut reply)
        .map_err(|e| e.to_string())?;
    match reply.trim_end() {
        "ok" => Ok(()),
        "" => Err("tonish closed the connection without a reply".into()),
        other => Err(other.to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_message_reads_back_from_its_name() {
        for &message in Message::value_variants() {
            assert_eq!(Message::parse(&message.name()), Some(message));
        }
        assert_eq!(Message::Quit.name(), "quit");
        assert_eq!(Message::parse("Quit"), None, "names are exact");
    }

    #[test]
    fn the_socket_lives_in_the_runtime_dir_named_after_the_display() {
        let at = |dir: Option<&str>, display: Option<&str>| {
            socket(dir.map(Into::into), display.map(Into::into))
        };
        let want = |p: &str| Some(PathBuf::from(p));
        assert_eq!(
            at(Some("/run/user/1000"), Some("wayland-1")),
            want("/run/user/1000/tonish-wayland-1.sock")
        );
        assert_eq!(
            at(Some("/run/user/1000"), Some("/run/user/1000/wayland-2")),
            want("/run/user/1000/tonish-wayland-2.sock")
        );
        assert_eq!(
            at(Some("/run/user/1000"), None),
            want("/run/user/1000/tonish-wayland-0.sock")
        );
        assert_eq!(at(None, Some("wayland-1")), None);
    }

    #[test]
    fn sending_without_a_tonish_names_the_socket() {
        let err = send(Path::new("/nonexistent/tonish.sock"), Message::Quit).unwrap_err();
        assert!(
            err.starts_with("no tonish answers at /nonexistent/tonish.sock"),
            "{err}"
        );
    }
}
