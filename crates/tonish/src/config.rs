//! The shipped file is compiled in, so that the built-in settings and the
//! file a first start writes can never disagree, and so that tonish needs no
//! file beside its binary, wherever it gets installed.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

use serde::de::Error;
use serde::{Deserialize, Deserializer};

use crate::shell::Orientation;

pub const DEFAULT: &str = include_str!("../../../assets/default-config.toml");

// deny_unknown_fields, so that a misspelt setting is an error instead of
// being ignored without a word.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub orientation: Orientation,
    #[serde(deserialize_with = "thickness")]
    pub thickness: f32,
}

/// The shipped file, so that the defaults live in one place, the file users
/// get. The tests make sure it parses.
impl Default for Config {
    fn default() -> Self {
        parse(DEFAULT).expect("the shipped default-config.toml parses")
    }
}

/// Sizes are cut to whole pixels, so a thickness under 1 would ask for size 0
/// along an axis the panel is not anchored on both sides of. That is a
/// layer-shell protocol error that gets tonish disconnected. Checked while
/// parsing, so that the error names the line.
fn thickness<'de, D: Deserializer<'de>>(d: D) -> Result<f32, D::Error> {
    let t = f32::deserialize(d)?;
    if t >= 1.0 {
        Ok(t)
    } else {
        Err(D::Error::custom(format!(
            "thickness {t} is less than 1 pixel"
        )))
    }
}

pub fn parse(text: &str) -> Result<Config, String> {
    toml::from_str(text).map_err(|e| e.to_string().trim_end().to_owned())
}

pub fn read(path: &Path) -> Result<Config, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    parse(&text).map_err(|e| format!("{}: {e}", path.display()))
}

/// `create_new`, so that a file the user wrote is never overwritten, not even
/// one written between a check for it and this call.
pub fn install(path: &Path) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?
        .write_all(DEFAULT.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell::{Anchor, Shell};

    #[test]
    fn the_shipped_settings_are_a_horizontal_panel_44_pixels_thick() {
        let want = Config {
            orientation: Orientation::Horizontal,
            thickness: 44.0,
        };
        assert_eq!(Config::default(), want);
    }

    #[test]
    fn a_vertical_panel_runs_down_the_left() {
        let shell = Shell::new(parse("orientation = \"vertical\"\nthickness = 30").unwrap());
        let panel = &shell.screens[0].panels[0];
        assert_eq!(
            (panel.anchor, panel.orientation, panel.thickness),
            (Anchor::Left, Orientation::Vertical, 30.0)
        );
    }

    #[test]
    fn both_settings_are_needed() {
        let err = parse("thickness = 44").unwrap_err();
        assert!(err.contains("missing field `orientation`"), "{err}");
    }

    #[test]
    fn a_misspelt_setting_is_an_error_naming_it_and_its_line() {
        let err = parse("orientation = \"vertical\"\nthicknes = 44\n").unwrap_err();
        assert!(err.contains("thicknes") && err.contains("line 2"), "{err}");
    }

    #[test]
    fn wrong_values_are_errors() {
        for (text, says) in [
            ("orientation = \"diagonal\"\nthickness = 44", "diagonal"),
            ("orientation = \"vertical\"\nthickness = 0", "less than 1"),
            ("orientation = \"vertical\"\nthickness = -4", "less than 1"),
            (
                "orientation = \"vertical\"\nthickness = \"wide\"",
                "thickness",
            ),
        ] {
            let err = parse(text).unwrap_err();
            assert!(err.contains(says), "{text}: {err}");
        }
    }

    #[test]
    fn errors_name_the_file() {
        let err = read(Path::new("/nonexistent/tonish.toml")).unwrap_err();
        assert!(err.starts_with("/nonexistent/tonish.toml: "), "{err}");
    }

    #[test]
    fn install_writes_the_shipped_file_and_never_overwrites_one() {
        let dir = std::env::temp_dir().join(format!("tonish-install-{}", std::process::id()));
        let path = dir.join("tonish").join("config.toml");
        install(&path).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), DEFAULT);

        fs::write(&path, "thickness = 30").unwrap();
        let again = install(&path).unwrap_err().kind();
        let kept = fs::read_to_string(&path).unwrap();
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(again, std::io::ErrorKind::AlreadyExists);
        assert_eq!(kept, "thickness = 30");
    }
}
