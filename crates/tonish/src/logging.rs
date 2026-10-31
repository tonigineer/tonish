//! Logs go to stderr and nowhere else: a terminal shows them, and a
//! compositor or systemd that starts tonish already keeps its stderr, with
//! the time of day and rotation, so a log file of tonish's own would only
//! duplicate that.

use std::io::IsTerminal;

use tracing::level_filters::LevelFilter;
use tracing::warn;
use tracing_subscriber::filter::Targets;
use tracing_subscriber::fmt::{self, time::uptime};
use tracing_subscriber::prelude::*;

/// `Targets` rather than the more capable `EnvFilter`, because `target=level`
/// per crate is all tonish needs, and `EnvFilter` would bring a regex engine
/// with it.
pub fn filter(var: Option<&str>) -> Result<Targets, String> {
    match var {
        None | Some("") => Ok(Targets::new().with_default(LevelFilter::INFO)),
        Some(text) => text.parse().map_err(|e| format!("TONISH_LOG={text}: {e}")),
    }
}

/// A broken `TONISH_LOG` falls back to the default instead of stopping
/// tonish, because the shell matters more than its log.
pub fn init() {
    let var = std::env::var("TONISH_LOG").ok();
    let (filter, broken) = match filter(var.as_deref()) {
        Ok(filter) => (filter, None),
        Err(e) => (filter(None).expect("the default parses"), Some(e)),
    };
    // Built from the registry rather than with `fmt()`, whose own level
    // ceiling of INFO would keep `TONISH_LOG=debug` from showing anything
    // more.
    tracing_subscriber::registry()
        .with(
            fmt::layer()
                .with_writer(std::io::stderr)
                // Seconds since the start rather than the time of day, which
                // the journal adds anyway; they show at a glance how long
                // starting took.
                .with_timer(uptime())
                // A compositor's log or the journal would show colours as
                // escape codes.
                .with_ansi(std::io::stderr().is_terminal()),
        )
        .with(filter)
        .init();
    if let Some(e) = broken {
        warn!("{e}; logging at the default level");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing::Level;

    #[test]
    fn without_tonish_log_info_and_above_is_logged() {
        let filter = filter(None).unwrap();
        assert!(filter.would_enable("engine::wayland", &Level::INFO));
        assert!(!filter.would_enable("engine::wayland", &Level::DEBUG));
        assert_eq!(
            format!("{:?}", super::filter(Some("")).unwrap()),
            format!("{filter:?}")
        );
    }

    #[test]
    fn tonish_log_sets_levels_per_crate() {
        let filter = filter(Some("engine=debug,warn")).unwrap();
        assert!(filter.would_enable("engine::wayland", &Level::DEBUG));
        assert!(!filter.would_enable("tonish::shell", &Level::INFO));
        assert!(filter.would_enable("tonish::shell", &Level::WARN));
    }

    #[test]
    fn a_broken_tonish_log_is_an_error_naming_it() {
        let err = filter(Some("engine=loud")).unwrap_err();
        assert!(err.starts_with("TONISH_LOG=engine=loud: "), "{err}");
    }
}
