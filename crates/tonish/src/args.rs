use clap::Parser;
use std::ffi::OsString;
use std::path::PathBuf;

use crate::shell::Shell;

#[derive(Debug, Parser)]
#[command(name = "tonish", version)]
pub struct Args {
    #[arg(short, long, value_name = "FILE")]
    config: Option<PathBuf>,
}

impl Args {
    fn config_path(&self) -> Option<PathBuf> {
        self.config.clone().or_else(|| {
            default_config(
                std::env::var_os("XDG_CONFIG_HOME"),
                std::env::var_os("HOME"),
            )
        })
    }
}

fn default_config(xdg_config_home: Option<OsString>, home: Option<OsString>) -> Option<PathBuf> {
    let folder = xdg_config_home
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| home.map(PathBuf::from))?;

    Some(folder.join("config").join("tonish").join("config.toml"))
}

pub fn load(args: Args) -> Result<Shell, String> {
    match args.config_path() {
        Some(path) if path.exists() => Ok(Shell::default()),
        Some(path) if args.config.is_some() => Err(format!(
            "tonish - config file does not exist: {}",
            path.display()
        )),
        _ => Ok(Shell::default()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;
    use clap::error::ErrorKind;

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
            &["--config", "a.toml"][..],
            &["--config=a.toml"],
            &["-c", "a.toml"],
        ] {
            assert_eq!(
                parse(args).unwrap().config,
                Some("a.toml".into()),
                "{args:?}"
            );
        }
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
            (&["run"], ErrorKind::UnknownArgument),
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
        assert_eq!(at(Some("/x"), Some("/h")), want("/x/tonish/config.toml"));
        assert_eq!(at(None, Some("/h")), want("/h/.config/tonish/config.toml"));
        assert_eq!(
            at(Some("rel"), Some("/h")),
            want("/h/.config/tonish/config.toml"),
            "relative is ignored"
        );
        assert_eq!(at(None, None), None);
    }

    #[test]
    fn a_settings_file_asked_for_must_exist() {
        let args = parse(&["--config", "/nonexistent/tonish.toml"]).unwrap();
        assert!(load(args).is_err());
    }
}
