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

/// A panel of thickness 0 would ask for size 0 along an axis it is not
/// anchored on both sides of, which is a layer-shell protocol error that
/// gets tonish disconnected. Checked while parsing, so that the error names
/// the line.
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
