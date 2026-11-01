use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::config::{self, Config};
use crate::ipc::{self, Message};
use crate::settings;
use crate::theme::Theme;
use engine::{
    self, App, Button, Key, Layer, On, Output, OutputId, Paint, PopupSpec, Redraw, Reply, Side,
    SurfaceId, SurfaceSpec,
};
use renderer::{Circle, Point, Rect, Scene, Shape};
use serde::Deserialize;
use tracing::{debug, info, warn};

/// The monitor name of a screen for every monitor that has no screen of
/// its own, so that a screen named after a monitor always wins over it.
pub const EVERY: &str = "*";
/// Logical pixels along the panel before its first component and after its
/// last.
pub const END: f32 = 8.0;
/// Logical pixels along the panel between two components.
pub const GAP: f32 = 4.0;

pub struct Shell {
    pub screens: Vec<Screen>,
    pub theme: Theme,
    /// Kept to tell a save that changes something from one that does not.
    config: Config,
    file: Option<PathBuf>,
    monitors: HashMap<OutputId, Option<String>>,
    redraw: Redraw,
    /// The settings panel, while it is open.
    popup: Option<PopupSpec>,
}

pub struct Screen {
    pub monitor: String,
    pub panels: Vec<Panel>,
}

pub struct Panel {
    pub anchor: Anchor,
    pub orientation: Orientation,
    /// Logical pixels across the panel.
    pub thickness: f32,
    pub components: Vec<Component>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Orientation {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Anchor {
    Top,
    Left,
    Right,
    Bottom,
    Center,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Component {
    Logo,
}

impl Default for Shell {
    fn default() -> Self {
        Self::new(Config::default())
    }
}

impl Shell {
    pub fn new(config: Config) -> Self {
        Self {
            screens: layout(config),
            theme: Theme::builtin(),
            config,
            file: None,
            monitors: HashMap::new(),
            redraw: Redraw::Nothing,
            popup: None,
        }
    }

    /// Watches the file it was read from, so that every edit applies without
    /// a restart. Only the built-in settings, which have no file, go
    /// unwatched.
    pub fn read(path: PathBuf) -> Result<Self, String> {
        let config = config::read(&path)?;
        info!(file = %path.display(), "settings read");
        Ok(Self {
            file: Some(path),
            ..Self::new(config)
        })
    }

    pub fn screen(&self, monitor: Option<&str>) -> Option<&Screen> {
        let named = monitor.and_then(|m| self.screens.iter().find(|s| s.monitor == m));
        named.or_else(|| self.screens.iter().find(|s| s.monitor == EVERY))
    }

    /// Against the stretch across the panel at the pointer, on the side
    /// away from the monitor's edge, so that the settings hang from the
    /// panel rather than cover it.
    fn settings(&self, output: OutputId, surface: SurfaceId, at: (f32, f32)) -> Option<PopupSpec> {
        let name = self.monitors.get(&output)?;
        let panel = self.screen(name.as_deref())?.panels.get(surface.0)?;
        let across = panel.thickness as i32;
        let (anchor, side) = match panel.orientation {
            Orientation::Horizontal => {
                let side = match panel.anchor {
                    Anchor::Bottom => Side::Above,
                    _ => Side::Below,
                };
                ((at.0 as i32, 0, 1, across), side)
            }
            Orientation::Vertical => {
                let side = match panel.anchor {
                    Anchor::Right => Side::Left,
                    _ => Side::Right,
                };
                ((0, at.1 as i32, across, 1), side)
            }
        };
        Some(PopupSpec {
            parent: (output, surface),
            anchor,
            side,
            size: settings::SIZE,
        })
    }
}

/// The layout stays in code until the settings file describes it; the file
/// only picks the orientation and thickness of the one panel. Each
/// orientation goes to an edge it spans the monitor along, so that the panel
/// reserves its room and windows stay clear of it.
fn layout(config: Config) -> Vec<Screen> {
    let anchor = match config.orientation {
        Orientation::Horizontal => Anchor::Top,
        Orientation::Vertical => Anchor::Left,
    };
    vec![Screen {
        monitor: EVERY.into(),
        panels: vec![Panel {
            anchor,
            orientation: config.orientation,
            thickness: config.thickness,
            components: vec![Component::Logo],
        }],
    }]
}

impl Panel {
    pub fn spans(&self) -> bool {
        match self.anchor {
            Anchor::Top | Anchor::Bottom => self.orientation == Orientation::Horizontal,
            Anchor::Left | Anchor::Right => self.orientation == Orientation::Vertical,
            Anchor::Center => false,
        }
    }

    pub fn content(&self) -> f32 {
        let lengths: f32 = self.components.iter().map(|c| c.length()).sum();
        let gaps = GAP * self.components.len().saturating_sub(1) as f32;
        END + lengths + gaps + END
    }

    pub fn size(&self, screen: (f32, f32)) -> (f32, f32) {
        let length = match (self.spans(), self.orientation) {
            (true, Orientation::Horizontal) => screen.0,
            (true, Orientation::Vertical) => screen.1,
            (false, _) => self.content(),
        };
        match self.orientation {
            Orientation::Horizontal => (length, self.thickness),
            Orientation::Vertical => (self.thickness, length),
        }
    }

    pub fn surface(&self, screen: (f32, f32)) -> SurfaceSpec {
        let (t, b, l, r) = (
            engine::Anchor::TOP,
            engine::Anchor::BOTTOM,
            engine::Anchor::LEFT,
            engine::Anchor::RIGHT,
        );
        let edge = match self.anchor {
            Anchor::Top => t,
            Anchor::Bottom => b,
            Anchor::Left => l,
            Anchor::Right => r,
            Anchor::Center => engine::Anchor::empty(),
        };
        let (w, h) = self.size(screen);
        let (anchor, size, exclusive) = match (self.spans(), self.orientation) {
            (true, Orientation::Horizontal) => (edge | l | r, (0, h as u32), h as i32),
            (true, Orientation::Vertical) => (edge | t | b, (w as u32, 0), w as i32),
            (false, _) => (edge, (w as u32, h as u32), 0),
        };
        SurfaceSpec {
            name: "panel",
            layer: Layer::Top,
            anchor,
            size,
            exclusive,
            margin: (0, 0, 0, 0),
        }
    }

    pub fn draw(&self, scene: &mut Scene, size: (f32, f32), t: &Theme) {
        scene.push(Rect {
            p0: Point::new(0.0, 0.0),
            p1: Point::new(size.0, size.1),
            color: t.surface,
        });
        let mut along = END;
        for c in &self.components {
            let middle = along + c.length() / 2.0;
            let center = match self.orientation {
                Orientation::Horizontal => Point::new(middle, self.thickness / 2.0),
                Orientation::Vertical => Point::new(self.thickness / 2.0, middle),
            };
            c.draw(scene, center, t);
            along += c.length() + GAP;
        }
    }
}

impl Component {
    pub fn length(self) -> f32 {
        match self {
            Component::Logo => 32.0,
        }
    }

    pub fn draw(self, scene: &mut Scene, center: Point, t: &Theme) {
        match self {
            Component::Logo => {
                let dot = Circle {
                    center,
                    r: 2.6,
                    color: t.primary,
                };
                scene.push(Circle { r: 8.0, ..dot }.outline(2.2, t.primary));
                scene.push(dot);
            }
        }
    }
}

impl App for Shell {
    fn surfaces(&mut self, out: &Output) -> Vec<SurfaceSpec> {
        self.monitors.insert(out.id, out.name.clone());
        match self.screen(out.name.as_deref()) {
            Some(screen) => screen.panels.iter().map(|p| p.surface(out.size)).collect(),
            None => Vec::new(),
        }
    }

    fn draw(&mut self, out: OutputId, surface: SurfaceId, paint: &mut Paint) {
        let Some(name) = self.monitors.get(&out) else {
            return;
        };
        if let Some(panel) = self
            .screen(name.as_deref())
            .and_then(|s| s.panels.get(surface.0))
        {
            panel.draw(paint.scene, paint.size, &self.theme);
        }
    }

    fn watched(&self) -> Vec<PathBuf> {
        self.file.iter().cloned().collect()
    }

    /// A broken file keeps the settings from before instead of stopping the
    /// shell, because editors save half-finished files, and the panels
    /// disappearing mid-edit would be worse than an edit not applying yet.
    fn changed(&mut self, path: &Path) -> bool {
        match config::read(path) {
            Ok(config) if config == self.config => {
                debug!(file = %path.display(), "saved without a change");
                false
            }
            Ok(config) => {
                info!(file = %path.display(), "settings changed");
                self.config = config;
                self.screens = layout(config);
                true
            }
            Err(e) => {
                warn!("keeping the settings from before: {e}");
                false
            }
        }
    }

    fn to_redraw(&mut self) -> Redraw {
        std::mem::take(&mut self.redraw)
    }

    /// Any other click on a panel closes the settings, as a click outside
    /// them would: the compositor closes them on its own only for clicks
    /// outside every surface of tonish.
    fn pressed(&mut self, on: On, at: (f32, f32), button: Button) {
        match on {
            On::Surface(output, surface) => {
                self.popup = match button {
                    Button::Right => self.settings(output, surface, at),
                    _ => None,
                };
            }
            // Hyprland sends a click outside the settings to them while they
            // hold the grab, with a point outside them.
            On::Popup => {
                let (w, h) = settings::SIZE;
                if !(0.0..w as f32).contains(&at.0) || !(0.0..h as f32).contains(&at.1) {
                    self.popup = None;
                }
            }
        }
    }

    fn key(&mut self, key: Key) {
        if key == Key::Escape {
            self.popup = None;
        }
    }

    fn popup(&self) -> Option<PopupSpec> {
        self.popup.clone()
    }

    fn popup_closed(&mut self) {
        self.popup = None;
    }

    fn draw_popup(&mut self, paint: &mut Paint) {
        if let Some(spec) = &self.popup {
            settings::draw(paint.scene, paint.size, spec.side, &self.theme);
        }
    }

    fn socket(&self) -> Option<PathBuf> {
        ipc::socket_from_env()
    }

    fn message(&mut self, text: &str) -> Reply {
        match Message::parse(text) {
            Some(Message::Quit) => Reply {
                text: "ok".into(),
                quit: true,
            },
            Some(Message::Redraw) => {
                self.redraw = Redraw::Everything;
                Reply {
                    text: "ok".into(),
                    quit: false,
                }
            }
            None => Reply {
                text: format!("error: unknown message {text:?}"),
                quit: false,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn output(id: u32, name: &str) -> Output {
        Output {
            id: OutputId(id),
            name: Some(name.into()),
            size: (1920.0, 1080.0),
            scale: 1,
        }
    }

    fn bar() -> On {
        On::Surface(OutputId(1), SurfaceId(0))
    }

    /// With its monitor known, as after the engine asked for its surfaces.
    fn shell_on_dp1(config: Config) -> Shell {
        let mut shell = Shell::new(config);
        shell.surfaces(&output(1, "DP-1"));
        shell
    }

    #[test]
    fn a_right_click_on_a_bar_along_the_top_hangs_the_settings_below_it() {
        let mut shell = shell_on_dp1(Config::default());
        assert_eq!(shell.popup(), None);
        shell.pressed(bar(), (120.5, 20.0), Button::Right);
        let want = PopupSpec {
            parent: (OutputId(1), SurfaceId(0)),
            anchor: (120, 0, 1, 44),
            side: Side::Below,
            size: settings::SIZE,
        };
        assert_eq!(shell.popup(), Some(want));
    }

    #[test]
    fn a_right_click_on_a_bar_down_the_left_hangs_the_settings_right_of_it() {
        let vertical = Config {
            orientation: Orientation::Vertical,
            thickness: 30.0,
        };
        let mut shell = shell_on_dp1(vertical);
        shell.pressed(bar(), (12.0, 300.0), Button::Right);
        let popup = shell.popup().unwrap();
        assert_eq!((popup.anchor, popup.side), ((0, 300, 30, 1), Side::Right));
    }

    #[test]
    fn a_click_outside_the_settings_that_reaches_them_closes_them() {
        let mut shell = shell_on_dp1(Config::default());
        shell.pressed(bar(), (120.0, 20.0), Button::Right);
        shell.pressed(On::Popup, (-105.9, 17.2), Button::Right);
        assert_eq!(shell.popup(), None);
    }

    #[test]
    fn another_click_on_a_panel_closes_the_settings() {
        let mut shell = shell_on_dp1(Config::default());
        shell.pressed(bar(), (120.0, 20.0), Button::Right);
        shell.pressed(bar(), (300.0, 20.0), Button::Left);
        assert_eq!(shell.popup(), None);
    }

    #[test]
    fn a_click_inside_the_settings_keeps_them_open() {
        let mut shell = shell_on_dp1(Config::default());
        shell.pressed(bar(), (120.0, 20.0), Button::Right);
        shell.pressed(On::Popup, (10.0, 10.0), Button::Left);
        assert!(shell.popup().is_some());
    }

    #[test]
    fn escape_closes_the_settings() {
        let mut shell = shell_on_dp1(Config::default());
        shell.pressed(bar(), (120.0, 20.0), Button::Right);
        shell.key(Key::Escape);
        assert_eq!(shell.popup(), None);
    }

    #[test]
    fn settings_the_compositor_closed_stay_closed() {
        let mut shell = shell_on_dp1(Config::default());
        shell.pressed(bar(), (120.0, 20.0), Button::Right);
        shell.popup_closed();
        assert_eq!(shell.popup(), None);
    }

    #[test]
    fn the_settings_cover_their_whole_surface() {
        let mut shell = shell_on_dp1(Config::default());
        shell.pressed(bar(), (120.0, 20.0), Button::Right);
        let mut scene = Scene::new();
        let mut paint = Paint {
            scene: &mut scene,
            size: (280.0, 180.0),
            scale: 1.0,
        };
        shell.draw_popup(&mut paint);
        let [p0, p1] = scene.shapes[0].bounds();
        assert!(p0.x <= 0.0 && p0.y <= 0.0 && p1.x >= 280.0 && p1.y >= 180.0);
    }

    #[test]
    fn by_default_a_panel_lies_along_the_top_keeping_its_room() {
        let mut shell = Shell::default();
        let specs = shell.surfaces(&output(1, "DP-1"));
        let top = &specs[0];
        assert_eq!(
            top.anchor,
            engine::Anchor::TOP | engine::Anchor::LEFT | engine::Anchor::RIGHT
        );
        assert_eq!((top.size, top.exclusive), ((0, 44), 44));
        assert_eq!(top.layer, Layer::Top);
    }

    #[test]
    fn a_monitor_named_in_the_settings_gets_its_own_panels() {
        let mut shell = Shell::default();
        shell.screens.push(Screen {
            monitor: "HDMI-A-1".into(),
            panels: vec![
                Panel {
                    anchor: Anchor::Left,
                    orientation: Orientation::Vertical,
                    thickness: 44.0,
                    components: vec![Component::Logo],
                },
                Panel {
                    anchor: Anchor::Center,
                    orientation: Orientation::Horizontal,
                    thickness: 32.0,
                    components: vec![Component::Logo, Component::Logo],
                },
            ],
        });
        let specs = shell.surfaces(&output(1, "HDMI-A-1"));
        assert_eq!(specs.len(), 2);
        let l = engine::Anchor::LEFT | engine::Anchor::TOP | engine::Anchor::BOTTOM;
        assert_eq!((specs[0].anchor, specs[0].size), (l, (44, 0)));
        // 84 is 8 + 32 + 4 + 32 + 8: both ends, two logos and the gap between them.
        assert_eq!(specs[1].anchor, engine::Anchor::empty());
        assert_eq!((specs[1].size, specs[1].exclusive), ((84, 32), 0));
        assert_eq!(
            shell.surfaces(&output(2, "DP-1")).len(),
            1,
            "every other monitor: the default"
        );
    }

    /// One file per test under the system temp dir, so that tests running in
    /// parallel never share one.
    fn settings(name: &str, text: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("tonish-{name}-{}.toml", std::process::id()));
        std::fs::write(&path, text).unwrap();
        path
    }

    #[test]
    fn only_a_shell_read_from_a_file_watches_it() {
        assert!(Shell::default().watched().is_empty());
        let path = settings("watched", config::DEFAULT);
        let watched = Shell::read(path.clone()).unwrap().watched();
        let _ = std::fs::remove_file(&path);
        assert_eq!(watched, [path]);
    }

    #[test]
    fn an_edit_rebuilds_the_panels() {
        let path = settings("edit", config::DEFAULT);
        let mut shell = Shell::read(path.clone()).unwrap();
        std::fs::write(&path, "orientation = \"vertical\"\nthickness = 30").unwrap();
        let rebuild = shell.changed(&path);
        let _ = std::fs::remove_file(&path);
        assert!(rebuild);
        let panel = &shell.screens[0].panels[0];
        assert_eq!((panel.anchor, panel.thickness), (Anchor::Left, 30.0));
    }

    #[test]
    fn a_save_that_changes_nothing_does_not_rebuild() {
        let path = settings("same", config::DEFAULT);
        let rebuild = Shell::default().changed(&path);
        let _ = std::fs::remove_file(&path);
        assert!(!rebuild);
    }

    #[test]
    fn a_broken_edit_keeps_the_settings_from_before() {
        let path = settings("broken", "orientation = \"vertical\"\nthickness = 0");
        let mut shell = Shell::default();
        let rebuild = shell.changed(&path);
        let _ = std::fs::remove_file(&path);
        assert!(!rebuild);
        assert_eq!(shell.screens[0].panels[0].anchor, Anchor::Top);
    }

    #[test]
    fn quit_is_answered_and_ends_the_shell() {
        let reply = Shell::default().message("quit");
        assert_eq!((reply.text.as_str(), reply.quit), ("ok", true));
    }

    #[test]
    fn redraw_is_answered_and_draws_everything_once() {
        let mut shell = Shell::default();
        assert_eq!(shell.to_redraw(), Redraw::Nothing);
        let reply = shell.message("redraw");
        assert_eq!((reply.text.as_str(), reply.quit), ("ok", false));
        assert_eq!(shell.to_redraw(), Redraw::Everything);
        assert_eq!(shell.to_redraw(), Redraw::Nothing, "taken by the first ask");
    }

    #[test]
    fn an_unknown_message_is_an_error_and_the_shell_goes_on() {
        let reply = Shell::default().message("reboot");
        assert!(reply.text.starts_with("error: "), "{}", reply.text);
        assert!(!reply.quit);
    }
}
