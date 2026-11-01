//! The settings are a Luau script that returns a table of them. It runs once
//! for every monitor, with `monitor` describing it, so that a setting can
//! follow the monitor, such as a bar a set distance shorter than the monitor
//! is wide.
//!
//! The shipped file is compiled in, so that the built-in settings and the
//! file a first start writes can never disagree, and so that tonish needs no
//! file beside its binary, wherever it gets installed.

use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::time::{Duration, Instant};

use engine::{Output, OutputId};
use mlua::{Lua, Table, Value, VmState};
use serde::de::Error;
use serde::{Deserialize, Deserializer};

use crate::shell::Orientation;

pub const DEFAULT: &str = include_str!("../../../assets/default-config.luau");
/// The settings' types for luau-lsp. They change with the settings, and
/// [`write_types`] keeps the copy beside the user's file current.
pub const TYPES: &str = include_str!("../../../assets/tonish.d.luau");

/// The global `Orientation`, which `tonish.d.luau` declares too: its
/// constants and the values they stand for, as [`Orientation`] reads them.
/// Constants rather than strings, so that luau-lsp completes them.
const ORIENTATION: [(&str, &str); 2] = [("Horizontal", "horizontal"), ("Vertical", "vertical")];

/// Settings take a few lines of arithmetic at most, so a script that runs
/// this long is stuck, and would freeze the shell.
const TIME: Duration = Duration::from_millis(500);
/// The same, for a script that keeps allocating.
const MEMORY: usize = 64 << 20;

/// Every field is required: the shipped file's table is merged under the
/// script's before this is read, so a setting the script leaves out has its
/// default here. deny_unknown_fields, here and in the sections, so that a
/// misspelt setting is an error instead of being ignored without a word.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// One width for the bar and every panel, so that the bar's line always
    /// runs on round the panels that hang from it.
    #[serde(deserialize_with = "border")]
    pub border: f32,
    pub bar: Bar,
    pub panel: Panels,
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bar {
    #[serde(deserialize_with = "orientation")]
    pub orientation: Orientation,
    #[serde(deserialize_with = "thickness")]
    pub thickness: f32,
    /// 0 spans the monitor.
    #[serde(deserialize_with = "length")]
    pub length: f32,
    #[serde(deserialize_with = "gap")]
    pub gap: f32,
    /// Top left, top right, bottom right, bottom left, as CSS and
    /// [`RoundedRect`](renderer::RoundedRect) order them.
    #[serde(deserialize_with = "radius")]
    pub radius: [f32; 4],
    /// 0 leaves out the concave corners.
    #[serde(deserialize_with = "inverse_radius")]
    pub inverse_radius: f32,
}

/// What every panel that opens from the bar has in common. A panel always
/// touches the bar, so it has no gap.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Panels {
    /// In the order of [`Bar::radius`].
    #[serde(deserialize_with = "radius")]
    pub radius: [f32; 4],
    #[serde(deserialize_with = "inverse_radius")]
    pub inverse_radius: f32,
}

/// For a script read before any monitor is known: it has to run for some
/// monitor, and a broken file should stop tonish at its start, not at its
/// first monitor.
pub fn stand_in() -> Output {
    Output {
        id: OutputId(0),
        name: None,
        size: (1920.0, 1080.0),
        scale: 1,
    }
}

/// The shipped file's settings, for the stand-in monitor.
impl Default for Config {
    fn default() -> Self {
        evaluate(DEFAULT, "=default-config.luau", &stand_in())
            .expect("the shipped default-config.luau runs")
    }
}

/// Runs the script for `monitor`. `name` is the chunk name its errors
/// start with, `@` and a path for a file.
pub fn evaluate(text: &str, name: &str, monitor: &Output) -> Result<Config, String> {
    let lua = state(monitor).map_err(|e| format!("cannot run the settings: {e}"))?;
    let shipped = run(&lua, DEFAULT, "=default-config.luau")?;
    let mine = run(&lua, text, name)?;
    merge(&shipped, &mine).map_err(message)?;
    // With the path to the setting, such as bar.thickness, because the table
    // no longer knows the lines its values came from.
    let settings = mlua::serde::Deserializer::new(Value::Table(shipped));
    serde_path_to_error::deserialize::<_, Config>(settings).map_err(|e| {
        let at = e.path().to_string();
        let what = match e.into_inner() {
            mlua::Error::DeserializeError(e) => e,
            e => e.to_string(),
        };
        let file = name.trim_start_matches(['@', '=']);
        if at == "." {
            format!("{file}: {what}")
        } else {
            format!("{file}: {at}: {what}")
        }
    })
}

/// A fresh state for every run, so that nothing one run leaves behind
/// reaches the next.
fn state(monitor: &Output) -> mlua::Result<Lua> {
    let lua = Lua::new();
    let m = lua.create_table()?;
    m.set("name", monitor.name.clone())?;
    m.set("width", monitor.size.0)?;
    m.set("height", monitor.size.1)?;
    m.set("scale", monitor.scale)?;
    // Before the sandbox, which makes the globals read-only.
    lua.globals().set("monitor", m)?;
    constants(&lua, "Orientation", &ORIENTATION)?;
    lua.sandbox(true)?;
    lua.set_memory_limit(MEMORY)?;
    let started = Instant::now();
    lua.set_interrupt(move |_| {
        if started.elapsed() > TIME {
            Err(mlua::Error::runtime(format!(
                "the settings ran for longer than {} ms",
                TIME.as_millis()
            )))
        } else {
            Ok(VmState::Continue)
        }
    });
    Ok(lua)
}

/// A misspelt constant is an error, where a plain table would give `nil`,
/// which reads as a setting left out and quietly takes its default. In Luau
/// rather than Rust, so that `error` can name the script's line: level 2 is
/// the code that looked the constant up.
fn constants(lua: &Lua, name: &str, values: &[(&str, &str)]) -> mlua::Result<()> {
    let table = lua.create_table()?;
    for (constant, value) in values {
        table.set(*constant, *value)?;
    }
    let known = values
        .iter()
        .map(|(c, _)| *c)
        .collect::<Vec<_>>()
        .join(", ");
    let guard: mlua::Function = lua
        .load(
            "local name, known = ...\n\
             return function(_, key)\n\
                 error(`{name} has no {key}, only {known}`, 2)\n\
             end",
        )
        .call((name, known))?;
    let meta = lua.create_table()?;
    meta.set("__index", guard)?;
    table.set_metatable(Some(meta))?;
    lua.globals().set(name, table)
}

fn run(lua: &Lua, text: &str, name: &str) -> Result<Table, String> {
    let file = name.trim_start_matches(['@', '=']);
    match lua.load(text).set_name(name).eval::<Value>() {
        Ok(Value::Table(table)) => Ok(table),
        Ok(other) => Err(format!(
            "{file}: returns {}, not a table of settings",
            other.type_name()
        )),
        Err(e) => {
            // The traceback only repeats the line for a script this short,
            // and would spread one log message over several lines.
            let e = message(e);
            let e = e.split("\nstack traceback:").next().unwrap_or_default();
            // Luau puts the file and line before its own errors, but not
            // before the time and memory limits' errors, which have none.
            if e.starts_with(file) {
                Err(e.into())
            } else {
                Err(format!("{file}: {e}"))
            }
        }
    }
}

/// Section by section, so that a section the script sets in part keeps the
/// shipped values of the rest.
fn merge(shipped: &Table, mine: &Table) -> mlua::Result<()> {
    for pair in mine.pairs::<Value, Value>() {
        let (key, value) = pair?;
        match (shipped.get::<Value>(key.clone())?, &value) {
            (Value::Table(section), Value::Table(settings)) => {
                for pair in settings.pairs::<Value, Value>() {
                    let (k, v) = pair?;
                    section.set(k, v)?;
                }
            }
            _ => shipped.set(key, value)?,
        }
    }
    Ok(())
}

/// Luau's own text, which names the file and the line, without mlua's
/// wording around it.
fn message(e: mlua::Error) -> String {
    match e {
        mlua::Error::SyntaxError { message, .. } | mlua::Error::RuntimeError(message) => message,
        mlua::Error::MemoryError(_) => {
            format!("the settings used more than {} MiB of memory", MEMORY >> 20)
        }
        mlua::Error::CallbackError { cause, .. } => message((*cause).clone()),
        e => e.to_string(),
    }
}

/// Also checks that it runs, for the stand-in monitor.
pub fn read(path: &Path) -> Result<String, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    evaluate(&text, &chunk(path), &stand_in())?;
    Ok(text)
}

/// The chunk name that makes Luau's errors start with the file's path.
pub fn chunk(path: &Path) -> String {
    format!("@{}", path.display())
}

/// Into `dir`, beside the settings, where an editor that opens them finds it.
/// Only when it differs, so that an unchanged copy keeps its time and the
/// editor has nothing to reload. Returns whether it wrote it.
pub fn write_types(dir: &Path) -> std::io::Result<bool> {
    let path = dir.join("tonish.d.luau");
    if fs::read_to_string(&path).is_ok_and(|text| text == TYPES) {
        return Ok(false);
    }
    fs::write(path, TYPES)?;
    Ok(true)
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

#[derive(Deserialize)]
#[serde(untagged)]
enum Radius {
    Every(f32),
    Each(HashMap<String, f32>),
}

/// In the order of [`Bar::radius`].
const CORNERS: [&str; 4] = ["top_left", "top_right", "bottom_right", "bottom_left"];

/// Sizes are cut to whole pixels, so a thickness under 1 would ask for size 0
/// along an axis the bar is not anchored on both sides of. That is a
/// layer-shell protocol error that gets tonish disconnected.
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

/// Between 0 and 1 the bar would get size 0 along its length, the same
/// protocol error as a thickness under 1.
fn length<'de, D: Deserializer<'de>>(d: D) -> Result<f32, D::Error> {
    let l = f32::deserialize(d)?;
    if l == 0.0 || l >= 1.0 {
        Ok(l)
    } else {
        Err(D::Error::custom(format!(
            "length {l} is neither 0 nor at least 1 pixel"
        )))
    }
}

/// One number for every corner, or a table of corners by name, so that
/// nobody has to remember an order. A corner the table leaves out is square
/// rather than the default, so that `{ bottom_left = 14, bottom_right = 14 }`
/// rounds those two alone. serde's own error for an untagged enum names
/// neither form, so this one does.
fn radius<'de, D: Deserializer<'de>>(d: D) -> Result<[f32; 4], D::Error> {
    let corners = match Radius::deserialize(d).map_err(|_| {
        D::Error::custom(
            "radius takes one number for every corner, or a table of them, such as \
             { top_left = 12, top_right = 12 }",
        )
    })? {
        Radius::Every(r) => [r; 4],
        Radius::Each(mut named) => {
            let corners = CORNERS.map(|c| named.remove(c).unwrap_or(0.0));
            if let Some(other) = named.keys().min() {
                return Err(D::Error::custom(format!(
                    "radius has no corner `{other}`, only `top_left`, `top_right`, \
                     `bottom_right` and `bottom_left`"
                )));
            }
            corners
        }
    };
    for r in corners {
        not_negative::<D::Error>("radius", r)?;
    }
    Ok(corners)
}

/// serde's own error names the strings, which the shipped file doesn't use.
fn orientation<'de, D: Deserializer<'de>>(d: D) -> Result<Orientation, D::Error> {
    Orientation::deserialize(d).map_err(|_| {
        let known: Vec<_> = ORIENTATION
            .iter()
            .map(|(c, _)| format!("Orientation.{c}"))
            .collect();
        D::Error::custom(format!("orientation takes {}", known.join(" or ")))
    })
}

fn gap<'de, D: Deserializer<'de>>(d: D) -> Result<f32, D::Error> {
    not_negative("gap", f32::deserialize(d)?)
}

fn inverse_radius<'de, D: Deserializer<'de>>(d: D) -> Result<f32, D::Error> {
    not_negative("inverse_radius", f32::deserialize(d)?)
}

fn border<'de, D: Deserializer<'de>>(d: D) -> Result<f32, D::Error> {
    not_negative("border", f32::deserialize(d)?)
}

fn not_negative<E: Error>(name: &str, value: f32) -> Result<f32, E> {
    if value >= 0.0 {
        Ok(value)
    } else {
        Err(E::custom(format!("{name} {value} is negative")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(text: &str) -> Result<Config, String> {
        evaluate(text, "=config.luau", &stand_in())
    }

    fn on(width: f32, name: &str) -> Output {
        Output {
            id: OutputId(1),
            name: Some(name.into()),
            size: (width, 1080.0),
            scale: 2,
        }
    }

    #[test]
    fn the_shipped_settings_are_a_plain_bar_and_rounded_panels() {
        let want = Config {
            border: 0.0,
            bar: Bar {
                orientation: Orientation::Horizontal,
                thickness: 44.0,
                length: 0.0,
                gap: 0.0,
                radius: [0.0; 4],
                inverse_radius: 0.0,
            },
            panel: Panels {
                radius: [14.0; 4],
                inverse_radius: 14.0,
            },
        };
        assert_eq!(Config::default(), want);
    }

    /// The defaults of the settings a script leaves out come from the
    /// shipped file, so it has to set every one.
    #[test]
    fn the_shipped_file_sets_every_setting() {
        let lua = state(&stand_in()).unwrap();
        let shipped = run(&lua, DEFAULT, "=default-config.luau").unwrap();
        let keys = |table: &Table| -> Vec<String> {
            let mut keys: Vec<String> = table
                .pairs::<String, Value>()
                .map(|p| p.unwrap().0)
                .collect();
            keys.sort();
            keys
        };
        assert_eq!(keys(&shipped), ["bar", "border", "panel"]);
        let bar = [
            "gap",
            "inverse_radius",
            "length",
            "orientation",
            "radius",
            "thickness",
        ];
        assert_eq!(keys(&shipped.get("bar").unwrap()), bar);
        assert_eq!(
            keys(&shipped.get("panel").unwrap()),
            ["inverse_radius", "radius"]
        );
    }

    #[test]
    fn the_types_name_every_setting_and_constant() {
        let lua = state(&stand_in()).unwrap();
        let shipped = run(&lua, DEFAULT, "=default-config.luau").unwrap();
        for pair in shipped.pairs::<String, Value>() {
            let (key, value) = pair.unwrap();
            assert!(TYPES.contains(&format!("{key}: ")), "{key}");
            if let Value::Table(section) = value {
                for pair in section.pairs::<String, Value>() {
                    let setting = pair.unwrap().0;
                    assert!(TYPES.contains(&format!("{setting}: ")), "{setting}");
                }
            }
        }
        for (constant, value) in ORIENTATION {
            assert!(TYPES.contains(&format!("{constant}: Orientation")));
            assert!(TYPES.contains(&format!("\"{value}\"")));
        }
    }

    #[test]
    fn a_constant_stands_for_its_value() {
        let vertical = "return { bar = { orientation = Orientation.Vertical } }";
        assert_eq!(
            settings(vertical).unwrap().bar.orientation,
            Orientation::Vertical
        );
        for (constant, _) in ORIENTATION {
            let text = format!("return {{ bar = {{ orientation = Orientation.{constant} }} }}");
            settings(&text).unwrap();
        }
    }

    #[test]
    fn a_misspelt_constant_is_an_error_naming_its_line() {
        let text = "local wide = true\nreturn { bar = { orientation = Orientation.Horizonal } }";
        let err = settings(text).unwrap_err();
        assert!(err.starts_with("config.luau:2: "), "{err}");
        assert!(
            err.ends_with("Orientation has no Horizonal, only Horizontal, Vertical"),
            "{err}"
        );
    }

    #[test]
    fn a_setting_left_out_takes_the_shipped_value() {
        let config = settings("return { border = 2.5, bar = { thickness = 30 } }").unwrap();
        let shipped = Config::default();
        assert_eq!((config.border, config.bar.thickness), (2.5, 30.0));
        assert_eq!(config.bar.orientation, shipped.bar.orientation);
        assert_eq!(config.panel, shipped.panel);
        assert_eq!(settings("return {}").unwrap(), shipped);
    }

    #[test]
    fn a_setting_can_follow_the_monitor() {
        let text = r#"
            local wide = monitor.width > 3000
            return {
                bar = {
                    length = monitor.width - 600,
                    thickness = if monitor.name == "DP-1" then 30 else 44,
                    radius = if wide then 12 else 6,
                },
            }
        "#;
        let config = evaluate(text, "=config.luau", &on(1920.0, "DP-1")).unwrap();
        assert_eq!((config.bar.length, config.bar.thickness), (1320.0, 30.0));
        assert_eq!(config.bar.radius, [6.0; 4]);
        let wide = evaluate(text, "=config.luau", &on(3440.0, "HDMI-A-1")).unwrap();
        assert_eq!((wide.bar.thickness, wide.bar.radius), (44.0, [12.0; 4]));
    }

    #[test]
    fn a_radius_is_one_for_every_corner_or_a_table_of_them() {
        let one = settings("return { bar = { radius = 6 } }").unwrap();
        assert_eq!(one.bar.radius, [6.0; 4]);
        let named = r#"return { panel = { radius = {
            top_left = 1, top_right = 2, bottom_right = 3, bottom_left = 4,
        } } }"#;
        assert_eq!(settings(named).unwrap().panel.radius, [1.0, 2.0, 3.0, 4.0]);
        let two = "return { panel = { radius = { bottom_left = 14, bottom_right = 14 } } }";
        assert_eq!(settings(two).unwrap().panel.radius, [0.0, 0.0, 14.0, 14.0]);
        let none = settings("return { panel = { radius = {} } }").unwrap();
        assert_eq!(none.panel.radius, [0.0; 4]);
    }

    #[test]
    fn a_misspelt_setting_is_an_error_naming_it_and_the_right_ones() {
        let err = settings("return { bar = { thicknes = 44 } }").unwrap_err();
        assert!(
            err.contains("`thicknes`") && err.contains("`thickness`"),
            "{err}"
        );
        let err = settings("return { thickness = 44 }").unwrap_err();
        assert!(err.contains("`bar`") && err.contains("`panel`"), "{err}");
    }

    #[test]
    fn broken_luau_is_an_error_naming_its_line() {
        let err = settings("return {\n  bar = { thickness = }\n}").unwrap_err();
        assert!(err.starts_with("config.luau:2"), "{err}");
        let err = settings("local x = monitor.nope + 1\nreturn {}").unwrap_err();
        assert!(err.starts_with("config.luau:1"), "{err}");
        assert!(!err.contains('\n'), "{err}");
    }

    #[test]
    fn a_script_that_returns_no_table_is_an_error() {
        let err = settings("return 5").unwrap_err();
        assert!(err.contains("not a table"), "{err}");
    }

    #[test]
    fn a_script_that_never_ends_is_stopped() {
        let started = Instant::now();
        let err = settings("while true do end\nreturn {}").unwrap_err();
        assert!(
            err.starts_with("config.luau: ") && err.contains("longer than"),
            "{err}"
        );
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn the_script_cannot_change_what_every_run_sees() {
        let err = settings("monitor.width = 1\nreturn {}").unwrap_err();
        assert!(
            err.contains("readonly") || err.contains("read-only"),
            "{err}"
        );
    }

    #[test]
    fn an_error_names_the_setting() {
        let err = settings(r#"return { bar = { thickness = "wide" } }"#).unwrap_err();
        assert!(err.starts_with("config.luau: bar.thickness: "), "{err}");
    }

    #[test]
    fn wrong_values_are_errors() {
        for (text, says) in [
            (
                "bar = { orientation = \"diagonal\" }",
                "Orientation.Horizontal or Orientation.Vertical",
            ),
            ("bar = { thickness = 0 }", "less than 1"),
            ("bar = { thickness = \"wide\" }", "thickness"),
            ("bar = { length = 0.5 }", "neither 0"),
            ("bar = { gap = -1 }", "gap -1 is negative"),
            ("bar = { radius = -2 }", "radius -2 is negative"),
            (
                "bar = { radius = { bottom_left = -4 } }",
                "radius -4 is negative",
            ),
            ("bar = { radius = { 1, 2, 3, 4 } }", "a table of them"),
            ("bar = { radius = { top = 2 } }", "no corner `top`"),
            (
                "bar = { inverse_radius = -3 }",
                "inverse_radius -3 is negative",
            ),
            ("border = -1", "border -1 is negative"),
            ("panel = { gap = 4 }", "gap"),
        ] {
            let err = settings(&format!("return {{ {text} }}")).unwrap_err();
            assert!(err.contains(says), "{text}: {err}");
        }
    }

    #[test]
    fn errors_name_the_file() {
        let err = read(Path::new("/nonexistent/tonish.luau")).unwrap_err();
        assert!(err.starts_with("/nonexistent/tonish.luau: "), "{err}");
        let err = evaluate("return 5", "@/x/config.luau", &stand_in()).unwrap_err();
        assert!(err.starts_with("/x/config.luau: "), "{err}");
    }

    #[test]
    fn install_writes_the_shipped_file_and_never_overwrites_one() {
        let dir = std::env::temp_dir().join(format!("tonish-install-{}", std::process::id()));
        let path = dir.join("tonish").join("config.luau");
        install(&path).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), DEFAULT);

        fs::write(&path, "return {}").unwrap();
        let again = install(&path).unwrap_err().kind();
        let kept = fs::read_to_string(&path).unwrap();
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(again, std::io::ErrorKind::AlreadyExists);
        assert_eq!(kept, "return {}");
    }

    #[test]
    fn the_types_are_written_when_they_differ() {
        let dir = std::env::temp_dir().join(format!("tonish-types-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("tonish.d.luau");
        let first = write_types(&dir).unwrap();
        let unchanged = write_types(&dir).unwrap();
        fs::write(&path, "-- an older tonish's types").unwrap();
        let older = write_types(&dir).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        let _ = fs::remove_dir_all(&dir);
        assert_eq!((first, unchanged, older), (true, false, true));
        assert_eq!(text, TYPES);
    }
}
