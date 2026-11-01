//! clap prints help, version and errors itself and exits (0 for help and
//! version, 2 for a wrong command line), so whoever gets an [`Args`] can rely
//! on a valid command line and never handles those cases.

use clap::{Parser, Subcommand};
use engine::Gpu;
use std::ffi::OsString;
use std::path::PathBuf;
use tracing::{info, warn};

use crate::config;
use crate::ipc::Message;
use crate::shell::Shell;

// clap turns the doc comments below into `--help`, so they are written for
// the people running tonish, not for its developers.

/// A desktop shell for Wayland: panels on every monitor.
#[derive(Debug, Parser)]
#[command(
    name = "tonish",
    version,
    after_help = "Logs go to stderr. Set TONISH_LOG to log more or less, as in TONISH_LOG=debug,\n\
                  or per part, as in TONISH_LOG=engine=debug,info."
)]
pub struct Args {
    // The default is only named in the help, not set with `default_value`:
    // it depends on the environment, and `None` lets `load` tell a file asked
    // for from the default one.
    /// Read the settings from FILE [default: $XDG_CONFIG_HOME/tonish/config.luau]
    #[arg(short, long, value_name = "FILE")]
    config: Option<PathBuf>,

    /// Draw every panel on the CPU, and never start the GPU, which saves memory
    #[arg(long)]
    disable_gpu: bool,

    #[command(subcommand)]
    pub command: Option<Command>,
}

impl Args {
    pub fn gpu(&self) -> Gpu {
        if self.disable_gpu {
            Gpu::Disabled
        } else {
            Gpu::Enabled
        }
    }
}

#[derive(Debug, Subcommand, PartialEq)]
pub enum Command {
    /// Send a message to the tonish running in this session
    Msg { message: Message },
}

/// Follows the XDG base directory spec, as sway and waybar do, including its
/// rule to ignore a relative `$XDG_CONFIG_HOME`.
///
/// The variables are passed in instead of read here, so that tests can try
/// any values: changing the real environment is `unsafe` since Rust 2024 and
/// races with tests running in parallel.
fn default_config(xdg_config_home: Option<OsString>, home: Option<OsString>) -> Option<PathBuf> {
    let folder = xdg_config_home
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| home.map(|h| PathBuf::from(h).join(".config")))?;

    Some(folder.join("tonish").join("config.luau"))
}

/// A file named with `--config` has to exist, because running with the
/// built-in settings instead would hide a typo in its path.
///
/// The default file is written on a first start, so that there is a file to
/// edit. If that fails, as on a read-only home, tonish still starts with the
/// built-in settings: a shell that does not come up is worse than one with
/// the default panel. A broken file, though, is an error, because quietly
/// falling back would look as if the edits had been ignored.
pub fn load(args: Args) -> Result<Shell, String> {
    if let Some(path) = args.config {
        return Shell::read(path);
    }
    let home = default_config(
        std::env::var_os("XDG_CONFIG_HOME"),
        std::env::var_os("HOME"),
    );
    let Some(path) = home else {
        warn!("HOME is not set, so the built-in settings apply");
        return Ok(Shell::default());
    };
    match config::install(&path) {
        Ok(()) => info!(file = %path.display(), "wrote the default settings"),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => {
            warn!(
                file = %path.display(),
                "cannot write the settings, so the built-in ones apply: {e}"
            );
            return Ok(Shell::default());
        }
    }
    if let Some(dir) = path.parent() {
        match config::write_types(dir) {
            Ok(true) => {
                info!(file = %dir.join("tonish.d.luau").display(), "wrote the settings' types")
            }
            Ok(false) => {}
            Err(e) => warn!(folder = %dir.display(), "cannot write the settings' types: {e}"),
        }
    }
    Shell::read(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;
    use clap::error::ErrorKind;

    /// `try_parse_from` returns what `parse` would print and exit with, so
    /// help, version and mistakes are checked by their [`ErrorKind`] without
    /// ending the test.
    fn parse(args: &[&str]) -> Result<Args, clap::Error> {
        Args::try_parse_from(std::iter::once("tonish").chain(args.iter().copied()))
    }

    #[test]
    fn the_arguments_are_declared_consistently() {
        Args::command().debug_assert();
    }

    #[test]
    fn without_arguments_no_settings_file_is_given() {
        assert_eq!(parse(&[]).unwrap().config, None);
    }

    #[test]
    fn a_settings_file_can_be_given_three_ways() {
        for args in [
            &["--config", "a.luau"][..],
            &["--config=a.luau"],
            &["-c", "a.luau"],
        ] {
            assert_eq!(
                parse(args).unwrap().config,
                Some("a.luau".into()),
                "{args:?}"
            );
        }
    }

    #[test]
    fn the_gpu_is_used_unless_disabled() {
        assert_eq!(parse(&[]).unwrap().gpu(), Gpu::Enabled);
        assert_eq!(parse(&["--disable-gpu"]).unwrap().gpu(), Gpu::Disabled);
    }

    #[test]
    fn help_and_version_are_answered_by_clap() {
        for (args, kind) in [
            ("-h", ErrorKind::DisplayHelp),
            ("--help", ErrorKind::DisplayHelp),
            ("-V", ErrorKind::DisplayVersion),
            ("--version", ErrorKind::DisplayVersion),
        ] {
            assert_eq!(parse(&[args]).unwrap_err().kind(), kind, "{args}");
        }
    }

    #[test]
    fn wrong_arguments_are_errors() {
        for (args, kind) in [
            (&["--bogus"][..], ErrorKind::UnknownArgument),
            (&["run"], ErrorKind::InvalidSubcommand),
            (&["--config"], ErrorKind::InvalidValue),
        ] {
            assert_eq!(parse(args).unwrap_err().kind(), kind, "{args:?}");
        }
    }

    #[test]
    fn the_default_settings_live_in_the_xdg_config_folder() {
        let at = |xdg: Option<&str>, home: Option<&str>| {
            default_config(xdg.map(Into::into), home.map(Into::into))
        };
        let want = |p: &str| Some(PathBuf::from(p));
        assert_eq!(at(Some("/x"), Some("/h")), want("/x/tonish/config.luau"));
        assert_eq!(at(None, Some("/h")), want("/h/.config/tonish/config.luau"));
        assert_eq!(
            at(Some("rel"), Some("/h")),
            want("/h/.config/tonish/config.luau"),
            "relative is ignored"
        );
        assert_eq!(at(None, None), None);
    }

    #[test]
    fn msg_takes_one_known_message() {
        let args = parse(&["msg", "quit"]).unwrap();
        assert_eq!(
            args.command,
            Some(Command::Msg {
                message: Message::Quit
            })
        );
        for (args, kind) in [
            (&["msg", "reboot"][..], ErrorKind::InvalidValue),
            (&["msg"], ErrorKind::MissingRequiredArgument),
        ] {
            assert_eq!(parse(args).unwrap_err().kind(), kind, "{args:?}");
        }
    }

    #[test]
    fn a_settings_file_asked_for_must_exist() {
        let args = parse(&["--config", "/nonexistent/tonish.luau"]).unwrap();
        assert!(load(args).is_err());
    }
}
