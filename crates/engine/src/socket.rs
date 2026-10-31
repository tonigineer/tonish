use std::fs;
use std::io::{ErrorKind, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};

use tracing::info;

/// Removes the socket file when dropped, so that it goes on every way out of
/// `run`, errors and panics included. Only SIGKILL leaves it behind, and
/// [`listen`] replaces it on the next start.
pub struct Socket {
    path: PathBuf,
}

impl Drop for Socket {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// Binding doubles as the check that no other instance runs in this
/// session, because two would put two sets of panels on every monitor. A
/// file nobody answers on was left by an instance that did not shut down
/// cleanly, and is replaced.
pub fn listen(path: &Path) -> Result<(UnixListener, Socket), String> {
    let listener = match UnixListener::bind(path) {
        Err(e) if e.kind() == ErrorKind::AddrInUse => {
            if UnixStream::connect(path).is_ok() {
                return Err(format!(
                    "already running: another instance listens at {}",
                    path.display()
                ));
            }
            info!(socket = %path.display(), "replacing a socket file nobody answers on");
            fs::remove_file(path).and_then(|()| UnixListener::bind(path))
        }
        bound => bound,
    }
    .map_err(|e| format!("{}: {e}", path.display()))?;
    // Nonblocking, because accepting runs on the event loop, which must not
    // wait for anyone.
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    info!(socket = %path.display(), "listening");
    Ok((
        listener,
        Socket {
            path: path.to_owned(),
        },
    ))
}

/// Longer messages are cut here, so that a client cannot make the shell
/// hold an unbounded buffer.
const MAX: usize = 4096;

/// Reads what has arrived so far. `None` means the message is not complete
/// yet and the loop should come back when more arrives; a client that never
/// finishes costs one idle event source, not a stalled loop.
pub fn read_line(mut stream: &UnixStream, line: &mut Vec<u8>) -> Option<String> {
    let mut chunk = [0; 256];
    loop {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                line.extend_from_slice(&chunk[..n]);
                if line.contains(&b'\n') || line.len() >= MAX {
                    break;
                }
            }
            Err(e) if e.kind() == ErrorKind::WouldBlock => return None,
            Err(_) => break,
        }
    }
    let text = String::from_utf8_lossy(line);
    Some(text.lines().next().unwrap_or("").trim().to_owned())
}

/// A client that has gone already makes the write fail, which is fine: no
/// one is left to tell.
pub fn reply(mut stream: &UnixStream, text: &str) {
    let _ = writeln!(stream, "{text}");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("engine-{name}-{}.sock", std::process::id()))
    }

    #[test]
    fn a_file_nobody_listens_on_is_replaced() {
        let path = path("stale");
        // A listener that goes without removing its file, as on a crash.
        drop(UnixListener::bind(&path).unwrap());
        assert!(path.exists());
        assert!(listen(&path).is_ok());
    }

    #[test]
    fn a_second_instance_is_refused() {
        let path = path("twice");
        let _first = listen(&path).unwrap();
        let err = listen(&path).err().unwrap();
        assert!(err.starts_with("already running"), "{err}");
    }

    #[test]
    fn the_file_goes_with_the_socket() {
        let path = path("gone");
        let (_listener, socket) = listen(&path).unwrap();
        drop(socket);
        assert!(!path.exists());
    }

    #[test]
    fn a_message_is_its_first_line_without_surrounding_space() {
        let (a, b) = UnixStream::pair().unwrap();
        b.set_nonblocking(true).unwrap();
        let mut line = Vec::new();
        assert_eq!(read_line(&b, &mut line), None, "nothing sent yet");
        (&a).write_all(b" qu").unwrap();
        assert_eq!(read_line(&b, &mut line), None, "no line end yet");
        (&a).write_all(b"it \nmore\n").unwrap();
        assert_eq!(read_line(&b, &mut line).as_deref(), Some("quit"));
    }

    #[test]
    fn a_message_ends_when_the_client_stops_writing() {
        let (a, b) = UnixStream::pair().unwrap();
        b.set_nonblocking(true).unwrap();
        (&a).write_all(b"quit").unwrap();
        drop(a);
        assert_eq!(read_line(&b, &mut Vec::new()).as_deref(), Some("quit"));
    }
}
