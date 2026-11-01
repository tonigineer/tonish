use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::config::{self, Config};
use crate::hang::{self, Edges};
use crate::hud::{self, Hud};
use crate::ipc::{self, Message};
use crate::motion::Spring;
use crate::settings;
use crate::theme::Theme;
use engine::{
    self, App, Button, Drawn, Key, Layer, On, Output, OutputId, Paint, PopupSpec, Redraw, Reply,
    Side, SurfaceId, SurfaceSpec,
};
use renderer::{Circle, Point, Scene, Shape};
use serde::Deserialize;
use tracing::{debug, info, warn};

/// Logical pixels along the panel before its first component and after its
/// last.
pub const END: f32 = 8.0;
/// Logical pixels along the panel between two components.
pub const GAP: f32 = 4.0;

pub struct Shell {
    pub theme: Theme,
    /// The settings script, which runs again for every monitor.
    source: String,
    /// The chunk name the script's errors start with.
    chunk: String,
    file: Option<PathBuf>,
    /// Every monitor the engine has asked about.
    monitors: HashMap<OutputId, Monitor>,
    redraw: Redraw,
    /// The settings panel, from when it opens until it has folded back.
    popup: Option<Popup>,
    hud: Hud,
    /// The surfaces changed other than through the settings, as when the
    /// figures were hidden, and the engine has yet to ask for them.
    reshape: bool,
}

/// The spring's settings are pleama-rs's: opening overshoots by about 5 %
/// and first reaches its size after about 0.2 s, closing reaches 99 % in
/// about 0.3 s without passing 0.
struct Popup {
    spec: PopupSpec,
    side: Side,
    /// 0 shut, 1 open.
    grow: Spring,
    /// Seconds since the card was all but back in the bar, and left waves
    /// on its edge: a clock rather than a spring, since the waves run at a
    /// steady speed.
    ripple: Option<f32>,
    /// When its last frame was drawn, which the next one steps the spring
    /// from. `None` while it rests, so that a motion starting after a pause
    /// starts with one frame's step rather than the whole pause.
    drawn: Option<Instant>,
}

impl Popup {
    fn closing(&self) -> bool {
        !self.spec.grab
    }

    fn moving(&self) -> bool {
        !self.grow.settled() || self.ripple.is_some_and(|t| t < settings::RIPPLE)
    }
}

/// What the settings made of a monitor, kept to tell a save that changes
/// something from one that does not.
pub struct Monitor {
    pub output: Output,
    pub config: Config,
    pub screen: Screen,
}

pub struct Screen {
    pub panels: Vec<Panel>,
}

pub struct Panel {
    pub anchor: Anchor,
    pub orientation: Orientation,
    /// Logical pixels across the panel.
    pub thickness: f32,
    /// Logical pixels along the panel. `None` spans the monitor along an
    /// edge, and fits the components anywhere else.
    pub length: Option<f32>,
    /// Logical pixels between the panel and its edge.
    pub gap: f32,
    pub edges: Edges,
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
        Self::new(config::DEFAULT.into())
    }
}

impl Shell {
    /// `source` is a settings script.
    pub fn new(source: String) -> Self {
        Self {
            theme: Theme::builtin(),
            source,
            chunk: "=config.luau".into(),
            file: None,
            monitors: HashMap::new(),
            redraw: Redraw::Nothing,
            popup: None,
            hud: Hud::default(),
            reshape: false,
        }
    }

    /// Watches the file it was read from, so that every edit applies without
    /// a restart. Only the built-in settings, which have no file, go
    /// unwatched.
    pub fn read(path: PathBuf) -> Result<Self, String> {
        let source = config::read(&path)?;
        info!(file = %path.display(), "settings read");
        Ok(Self {
            chunk: config::chunk(&path),
            file: Some(path),
            ..Self::new(source)
        })
    }

    pub fn monitor(&self, id: OutputId) -> Option<&Monitor> {
        self.monitors.get(&id)
    }

    /// Out of the panel's inner edge at the pointer, on the side away from
    /// the monitor's edge, so that the settings hang from the panel rather
    /// than cover it. The card is centred on the pointer, but no nearer the
    /// panel's ends than its straight inner edge allows, with room for the
    /// concave corners: past that, they would curve out of nothing. On a
    /// panel too short for that, the card starts at the panel's start.
    fn settings(&self, output: OutputId, surface: SurfaceId, at: (f32, f32)) -> Option<Popup> {
        let monitor = self.monitors.get(&output)?;
        let (out, config) = (&monitor.output, &monitor.config);
        let panel = monitor.screen.panels.get(surface.0)?;
        let side = match (panel.orientation, panel.anchor) {
            (Orientation::Horizontal, Anchor::Bottom) => Side::Above,
            (Orientation::Horizontal, _) => Side::Below,
            (Orientation::Vertical, Anchor::Right) => Side::Left,
            (Orientation::Vertical, _) => Side::Right,
        };
        let room = settings::room(side, config);
        let (size, card) = (panel.size(out.size), settings::CARD);
        let (length, card, pointer) = match panel.orientation {
            Orientation::Horizontal => (size.0, card.0, at.0),
            Orientation::Vertical => (size.1, card.1, at.1),
        };
        // The bar's far corners, at either end of the edge the card hangs
        // from.
        let r = panel.edges.radii;
        let (first, last) = match panel.side() {
            Side::Below => (r[3], r[2]),
            Side::Above => (r[0], r[1]),
            Side::Right => (r[1], r[2]),
            Side::Left => (r[0], r[3]),
        };
        let (lo, hi) = (
            panel.room() + first + room,
            length - panel.room() - last - room - card,
        );
        let start = pointer - card / 2.0;
        let start = if lo <= hi { start.clamp(lo, hi) } else { lo };
        let along = (start - room).max(0.0);
        // Over the bar's border, so that its line runs into the settings'
        // instead of beside it.
        let overlap = panel.edges.border.ceil();
        let size = settings::size(side, config, overlap);
        let (x, y) = panel.origin(out.size);
        let edge = panel.thickness - overlap;
        let at = match side {
            Side::Below => (x + along, y + edge),
            Side::Right => (x + edge, y + along),
            Side::Above => (x + along, y + overlap - size.1 as f32),
            Side::Left => (x + overlap - size.0 as f32, y + along),
        };
        let [p0, p1] = settings::card(side, config, overlap);
        let (x0, y0) = (p0.x.floor() as i32, p0.y.floor() as i32);
        let input = (x0, y0, p1.x.ceil() as i32 - x0, p1.y.ceil() as i32 - y0);
        let mut grow = Spring::new(0.0);
        grow.aim(1.0, 260.0, 0.68);
        Some(Popup {
            spec: PopupSpec {
                parent: (output, surface),
                at: (at.0 as i32, at.1 as i32),
                size,
                grab: true,
                input: Some(input),
            },
            side,
            grow,
            drawn: None,
            ripple: None,
        })
    }

    /// The figures come after a monitor's panels in its list of surfaces.
    fn is_hud(&self, on: On) -> bool {
        match on {
            On::Surface(out, SurfaceId(si)) => {
                self.hud.shown
                    && self
                        .monitors
                        .get(&out)
                        .is_some_and(|m| si == m.screen.panels.len())
            }
            On::Popup => false,
        }
    }

    /// Folds the settings back into the bar. They let clicks and the
    /// keyboard go at once, while they fold.
    fn close(&mut self) {
        let Some(p) = &mut self.popup else {
            return;
        };
        if p.closing() {
            return;
        }
        p.spec.grab = false;
        p.spec.input = None;
        p.grow.aim(0.0, 420.0, 1.0);
        // Its first frame of folding back; the frames after it follow from
        // `animating`.
        match &mut self.redraw {
            Redraw::Everything => {}
            Redraw::Surfaces(on) => on.push(On::Popup),
            nothing => *nothing = Redraw::Surfaces(vec![On::Popup]),
        }
    }
}

/// The layout stays in code until the settings describe it; they only shape
/// the one panel. Each orientation goes to an edge it lies along, so that
/// the panel reserves its room and windows stay clear of it.
fn layout(config: Config) -> Screen {
    let bar = config.bar;
    let anchor = match bar.orientation {
        Orientation::Horizontal => Anchor::Top,
        Orientation::Vertical => Anchor::Left,
    };
    let mut panel = Panel {
        anchor,
        orientation: bar.orientation,
        thickness: bar.thickness,
        length: (bar.length > 0.0).then_some(bar.length),
        gap: bar.gap,
        edges: Edges {
            radii: bar.radius,
            inverse_radius: bar.inverse_radius,
            border: config.border,
        },
        components: vec![Component::Logo],
    };
    panel.edges = panel.edges.fit(bar.thickness, panel.side());
    Screen {
        panels: vec![panel],
    }
}

fn name(out: &Output) -> &str {
    out.name.as_deref().unwrap_or("unnamed")
}

impl Panel {
    /// Along the edge it is anchored to, as a bar is, rather than across
    /// it or in the middle of the monitor. Only such a panel reserves room,
    /// because layer-shell only lets a surface anchored to an edge do so.
    pub fn along_edge(&self) -> bool {
        match self.anchor {
            Anchor::Top | Anchor::Bottom => self.orientation == Orientation::Horizontal,
            Anchor::Left | Anchor::Right => self.orientation == Orientation::Vertical,
            Anchor::Center => false,
        }
    }

    pub fn spans(&self) -> bool {
        self.along_edge() && self.length.is_none()
    }

    /// Touching its edge without spanning it, so that its corners there
    /// curve out into the edge.
    pub fn hangs(&self) -> bool {
        self.along_edge() && !self.spans() && self.gap == 0.0 && self.edges.inverse_radius > 0.0
    }

    /// The side of its edge the panel is on.
    pub fn side(&self) -> Side {
        match (self.anchor, self.orientation) {
            (Anchor::Bottom, _) => Side::Above,
            (Anchor::Right, _) => Side::Left,
            (Anchor::Left, _) | (Anchor::Center, Orientation::Vertical) => Side::Right,
            (Anchor::Top, _) | (Anchor::Center, Orientation::Horizontal) => Side::Below,
        }
    }

    /// Room at each end of the surface for the concave corners.
    pub fn room(&self) -> f32 {
        if self.hangs() {
            self.edges.inverse_radius
        } else {
            0.0
        }
    }

    pub fn content(&self) -> f32 {
        let lengths: f32 = self.components.iter().map(|c| c.length()).sum();
        let gaps = GAP * self.components.len().saturating_sub(1) as f32;
        END + lengths + gaps + END
    }

    /// The surface's size: the panel, and the room for its concave corners.
    pub fn size(&self, screen: (f32, f32)) -> (f32, f32) {
        let length = match (self.spans(), self.orientation) {
            (true, Orientation::Horizontal) => screen.0,
            (true, Orientation::Vertical) => screen.1,
            (false, _) => self.length.unwrap_or_else(|| self.content()) + 2.0 * self.room(),
        };
        match self.orientation {
            Orientation::Horizontal => (length, self.thickness),
            Orientation::Vertical => (self.thickness, length),
        }
    }

    /// Where the compositor puts the panel's surface on the monitor, as its
    /// top-left corner: against its edge, past its gap, and centred along it
    /// as wlroots centres a surface anchored to one edge, in whole pixels.
    /// Another program's surface keeping room on the same edge would move
    /// the panel without this knowing.
    pub fn origin(&self, screen: (f32, f32)) -> (f32, f32) {
        let (w, h) = self.size(screen);
        let (w, h) = (w as u32 as i32, h as u32 as i32);
        let (sw, sh) = (screen.0 as i32, screen.1 as i32);
        let centre = |total: i32, length: i32| ((total - length) / 2) as f32;
        let (x, y) = (centre(sw, w), centre(sh, h));
        let g = self.gap;
        match (self.anchor, self.spans()) {
            (Anchor::Top, true) | (Anchor::Left, true) => (0.0, 0.0),
            (Anchor::Top, false) => (x, g),
            (Anchor::Bottom, true) => (0.0, (sh - h) as f32 - g),
            (Anchor::Bottom, false) => (x, (sh - h) as f32 - g),
            (Anchor::Left, false) => (g, y),
            (Anchor::Right, true) => ((sw - w) as f32 - g, 0.0),
            (Anchor::Right, false) => ((sw - w) as f32 - g, y),
            (Anchor::Center, _) => (x, y),
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
        let (anchor, size) = match (self.spans(), self.orientation) {
            (true, Orientation::Horizontal) => (edge | l | r, (0, h as u32)),
            (true, Orientation::Vertical) => (edge | t | b, (w as u32, 0)),
            (false, _) => (edge, (w as u32, h as u32)),
        };
        // The compositor adds the margin to the room the panel reserves.
        let exclusive = if self.along_edge() {
            self.thickness as i32
        } else {
            0
        };
        let g = self.gap as i32;
        let margin = match self.anchor {
            Anchor::Top => (g, 0, 0, 0),
            Anchor::Right => (0, g, 0, 0),
            Anchor::Bottom => (0, 0, g, 0),
            Anchor::Left => (0, 0, 0, g),
            Anchor::Center => (0, 0, 0, 0),
        };
        SurfaceSpec {
            name: "panel",
            layer: Layer::Top,
            anchor,
            size,
            exclusive,
            margin,
            input: true,
        }
    }

    pub fn draw(&self, scene: &mut Scene, size: (f32, f32), t: &Theme) {
        let room = self.room();
        let (p0, p1) = match self.orientation {
            Orientation::Horizontal => (Point::new(room, 0.0), Point::new(size.0 - room, size.1)),
            Orientation::Vertical => (Point::new(0.0, room), Point::new(size.0, size.1 - room)),
        };
        hang::draw(
            scene,
            [p0, p1],
            self.side(),
            self.hangs(),
            self.edges,
            0.0,
            t.surface,
            t.outline,
        );
        let mut along = room + END;
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
    /// Runs the settings for a monitor it doesn't know yet. For one it
    /// knows, `changed` has run them already.
    fn surfaces(&mut self, out: &Output) -> Vec<SurfaceSpec> {
        let known = self.monitors.get(&out.id).is_some_and(|m| m.output == *out);
        if !known {
            let config = match config::evaluate(&self.source, &self.chunk, out) {
                Ok(config) => config,
                // A script that fails for one monitor only, such as one that
                // does arithmetic with a monitor's missing name.
                Err(e) => {
                    warn!(monitor = %name(out), "the monitor gets the default settings: {e}");
                    Config::default()
                }
            };
            let monitor = Monitor {
                output: out.clone(),
                config,
                screen: layout(config),
            };
            self.monitors.insert(out.id, monitor);
        }
        let screen = &self.monitors[&out.id].screen;
        let mut surfaces: Vec<_> = screen.panels.iter().map(|p| p.surface(out.size)).collect();
        if self.hud.shown {
            surfaces.push(Hud::surface());
        }
        surfaces
    }

    fn draw(&mut self, out: OutputId, surface: SurfaceId, paint: &mut Paint) {
        let Some(monitor) = self.monitors.get(&out) else {
            return;
        };
        if let Some(panel) = monitor.screen.panels.get(surface.0) {
            panel.draw(paint.scene, paint.size, &self.theme);
        } else if self.is_hud(On::Surface(out, surface)) {
            let output = monitor.output.clone();
            self.hud
                .draw(paint.scene, &output, paint.scale, paint.time, &self.theme);
        }
    }

    fn watched(&self) -> Vec<PathBuf> {
        self.file.iter().cloned().collect()
    }

    /// A broken file keeps the settings from before instead of stopping the
    /// shell, because editors save half-finished files, and the panels
    /// disappearing mid-edit would be worse than an edit not applying yet.
    /// It has to run for every monitor, so that no monitor is left with the
    /// settings from before while the others take the new ones.
    fn changed(&mut self, path: &Path) -> bool {
        let source = match config::read(path) {
            Ok(source) => source,
            Err(e) => {
                warn!("keeping the settings from before: {e}");
                return false;
            }
        };
        let chunk = config::chunk(path);
        let mut configs = Vec::new();
        for (id, m) in &self.monitors {
            match config::evaluate(&source, &chunk, &m.output) {
                Ok(config) => configs.push((*id, config)),
                Err(e) => {
                    warn!(monitor = %name(&m.output), "keeping the settings from before: {e}");
                    return false;
                }
            }
        }
        (self.source, self.chunk) = (source, chunk);
        if configs.iter().all(|(id, c)| self.monitors[id].config == *c) {
            debug!(file = %path.display(), "saved without a change");
            return false;
        }
        info!(file = %path.display(), "settings changed");
        for (id, config) in configs {
            if let Some(m) = self.monitors.get_mut(&id) {
                m.config = config;
                m.screen = layout(config);
            }
        }
        // A change that leaves a surface its size, such as of the radius
        // alone, brings no configure to draw it again.
        self.redraw = Redraw::Everything;
        true
    }

    fn to_redraw(&mut self) -> Redraw {
        std::mem::take(&mut self.redraw)
    }

    /// Any other click on a panel closes the settings, as a click outside
    /// them would: the compositor ends their grab on its own only for clicks
    /// outside every surface of tonish.
    fn pressed(&mut self, on: On, at: (f32, f32), button: Button) {
        match on {
            On::Surface(output, surface) if button == Button::Right => {
                self.popup = self.settings(output, surface, at);
            }
            On::Surface(..) => self.close(),
            // Hyprland sends a click outside the settings to them while they
            // hold the grab, with a point outside them. The room beside the
            // card, where only the concave corners are, counts as outside.
            On::Popup => {
                let Some(p) = &self.popup else {
                    return;
                };
                let Some(m) = self.monitors.get(&p.spec.parent.0) else {
                    return;
                };
                let overlap = m.config.border.ceil();
                let [p0, p1] = settings::card(p.side, &m.config, overlap);
                if !(p0.x..p1.x).contains(&at.0) || !(p0.y..p1.y).contains(&at.1) {
                    self.close();
                }
            }
        }
    }

    fn key(&mut self, key: Key) {
        if key == Key::Escape {
            self.close();
        }
    }

    fn popup(&self) -> Option<PopupSpec> {
        self.popup.as_ref().map(|p| p.spec.clone())
    }

    fn popup_dismissed(&mut self) {
        self.close();
    }

    fn popup_closed(&mut self) {
        self.popup = None;
    }

    /// The figures are drawn again and again for as long as they show, at
    /// their own few frames a second.
    fn animating(&self, on: On) -> bool {
        match on {
            On::Popup => self.popup.as_ref().is_some_and(Popup::moving),
            on => self.is_hud(on),
        }
    }

    fn max_fps(&self, on: On) -> Option<f32> {
        match on {
            On::Popup => {
                let parent = self.popup.as_ref()?.spec.parent.0;
                let fps = self.monitors.get(&parent)?.config.animation.max_fps;
                (fps > 0.0).then_some(fps)
            }
            on => self.is_hud(on).then_some(hud::FPS),
        }
    }

    fn to_reshape(&mut self) -> bool {
        std::mem::take(&mut self.reshape)
    }

    /// The figures' own frames aren't counted, or they would count
    /// themselves.
    fn drew(&mut self, on: On, drawn: Drawn) {
        if !self.is_hud(on) {
            self.hud.drew(on, drawn, Instant::now());
        }
    }

    fn gpu_started(&mut self, device: &str) {
        self.hud.gpu_started(device);
    }

    /// Steps the spring to the frame's time first, so that every frame
    /// shows where the card is when it is drawn. Once it has folded back,
    /// the settings are gone, and the engine destroys their surface.
    fn draw_popup(&mut self, paint: &mut Paint) {
        let Some(p) = &mut self.popup else {
            return;
        };
        let dt = p
            .drawn
            .map_or(1.0 / 60.0, |t| paint.time.duration_since(t).as_secs_f32());
        p.grow.step(dt);
        if let Some(t) = &mut p.ripple {
            // Steps of at most 0.1 s, as the spring's, so that the waves
            // resume rather than jump after a pause.
            *t += dt.min(0.1);
        } else if p.closing() && p.grow.value < 0.06 {
            p.ripple = Some(0.0);
        }
        p.drawn = p.moving().then_some(paint.time);
        if let Some(m) = self.monitors.get(&p.spec.parent.0) {
            let overlap = m.config.border.ceil();
            settings::draw(
                paint.scene,
                p.side,
                &m.config,
                overlap,
                p.grow.value,
                p.ripple,
                &self.theme,
            );
        }
        if p.closing() && !p.moving() {
            self.popup = None;
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
            Some(Message::Hud) => {
                self.hud.shown = !self.hud.shown;
                self.reshape = true;
                Reply {
                    text: if self.hud.shown { "shown" } else { "hidden" }.into(),
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
    fn shell_on_dp1(source: &str) -> Shell {
        let mut shell = Shell::new(source.into());
        shell.surfaces(&output(1, "DP-1"));
        shell
    }

    const SHIPPED: &str = "return {}";

    /// The settings' frames at 60 Hz for `seconds`, the last one left in the
    /// returned scene.
    fn play(shell: &mut Shell, seconds: f32) -> Scene {
        let mut scene = Scene::new();
        let start = Instant::now();
        for i in 0..=(seconds * 60.0) as u32 {
            let Some(spec) = shell.popup() else {
                break;
            };
            scene.clear();
            let mut paint = Paint {
                scene: &mut scene,
                size: (spec.size.0 as f32, spec.size.1 as f32),
                scale: 1.0,
                time: start + std::time::Duration::from_secs_f32(i as f32 / 60.0),
            };
            shell.draw_popup(&mut paint);
        }
        scene
    }

    fn folding(shell: &Shell) -> bool {
        shell.popup().is_some_and(|p| !p.grab && p.input.is_none())
    }

    #[test]
    fn a_right_click_on_a_bar_along_the_top_hangs_the_settings_below_it() {
        let mut shell = shell_on_dp1(SHIPPED);
        assert_eq!(shell.popup(), None);
        shell.pressed(bar(), (400.5, 20.0), Button::Right);
        // The card centred on the click, and the popup reaching the concave
        // corners' room further, below the bar along the monitor's top.
        let config = Config::default();
        let room = settings::room(Side::Below, &config);
        let start = 400.5 - settings::CARD.0 / 2.0 - room;
        let size = settings::size(Side::Below, &config, 0.0);
        let want = PopupSpec {
            parent: (OutputId(1), SurfaceId(0)),
            at: (start as i32, 44),
            size,
            grab: true,
            input: Some((
                room as i32,
                0,
                settings::CARD.0 as i32,
                settings::CARD.1 as i32,
            )),
        };
        assert_eq!(shell.popup(), Some(want));
    }

    #[test]
    fn a_right_click_on_a_bar_down_the_left_hangs_the_settings_right_of_it() {
        let mut shell =
            shell_on_dp1("return { bar = { orientation = Orientation.Vertical, thickness = 30 } }");
        shell.pressed(bar(), (12.0, 300.0), Button::Right);
        let popup = shell.popup().unwrap();
        let room = settings::room(Side::Right, &Config::default());
        let start = (300.0 - settings::CARD.1 / 2.0 - room) as i32;
        assert_eq!(popup.at, (30, start));
    }

    #[test]
    fn hanging_settings_cover_the_bars_border() {
        let mut shell = shell_on_dp1("return { border = 1.5 }");
        shell.pressed(bar(), (400.0, 20.0), Button::Right);
        assert_eq!(shell.popup().unwrap().at.1, 44 - 2);
    }

    #[test]
    fn near_the_panels_start_the_settings_stay_on_it() {
        let mut shell = shell_on_dp1(SHIPPED);
        shell.pressed(bar(), (5.0, 20.0), Button::Right);
        assert_eq!(shell.popup().unwrap().at, (0, 44));
    }

    /// On a bar 600 long, with its concave corners and rounded ends, and the
    /// settings' own concave corners of 14.
    #[test]
    fn near_a_shorter_bars_end_the_settings_stay_under_its_straight_edge() {
        let mut shell =
            shell_on_dp1("return { bar = { length = 600, radius = 12, inverse_radius = 20 } }");
        // The bar's surface is 640 long, in the middle of the monitor from
        // 640: 20 of room at each end, then 12 of rounded corner, so its
        // straight inner edge runs from 32 to 608 on it.
        shell.pressed(bar(), (600.0, 20.0), Button::Right);
        let start = (shell.popup().unwrap().at.0 - 640) as f32 + 14.0;
        assert_eq!(start, 608.0 - 14.0 - settings::CARD.0);
        shell.pressed(bar(), (21.0, 20.0), Button::Right);
        assert_eq!(shell.popup().unwrap().at.0 - 640, 32);
    }

    #[test]
    fn on_a_bar_too_short_for_them_the_settings_start_at_its_start() {
        let mut shell = shell_on_dp1("return { bar = { length = 200 } }");
        shell.pressed(bar(), (150.0, 20.0), Button::Right);
        assert_eq!(shell.popup().unwrap().at.0, (1920 - 200) / 2);
    }

    #[test]
    fn a_bar_along_the_bottom_has_the_settings_above_it() {
        let mut shell = shell_on_dp1(SHIPPED);
        let mut monitor = shell.monitors.remove(&OutputId(1)).unwrap();
        monitor.screen.panels[0].anchor = Anchor::Bottom;
        shell.monitors.insert(OutputId(1), monitor);
        shell.pressed(bar(), (400.0, 20.0), Button::Right);
        let popup = shell.popup().unwrap();
        assert_eq!(popup.at.1 + popup.size.1 as i32, 1080 - 44);
    }

    #[test]
    fn a_click_beside_the_card_folds_the_settings_back() {
        let mut shell = shell_on_dp1(SHIPPED);
        shell.pressed(bar(), (400.0, 20.0), Button::Right);
        let room = settings::room(Side::Below, &Config::default());
        shell.pressed(On::Popup, (room / 2.0, 100.0), Button::Left);
        assert!(folding(&shell));
    }

    #[test]
    fn a_click_outside_the_settings_that_reaches_them_folds_them_back() {
        let mut shell = shell_on_dp1(SHIPPED);
        shell.pressed(bar(), (400.0, 20.0), Button::Right);
        shell.pressed(On::Popup, (-105.9, 17.2), Button::Right);
        assert!(folding(&shell));
    }

    #[test]
    fn another_click_on_a_panel_folds_the_settings_back() {
        let mut shell = shell_on_dp1(SHIPPED);
        shell.pressed(bar(), (400.0, 20.0), Button::Right);
        shell.pressed(bar(), (300.0, 20.0), Button::Left);
        assert!(folding(&shell));
    }

    #[test]
    fn a_click_inside_the_settings_keeps_them_open() {
        let mut shell = shell_on_dp1(SHIPPED);
        shell.pressed(bar(), (400.0, 20.0), Button::Right);
        let room = settings::room(Side::Below, &Config::default());
        shell.pressed(On::Popup, (room + 10.0, 10.0), Button::Left);
        assert!(shell.popup().unwrap().grab);
    }

    #[test]
    fn escape_and_a_click_outside_tonish_fold_the_settings_back() {
        let mut shell = shell_on_dp1(SHIPPED);
        shell.pressed(bar(), (400.0, 20.0), Button::Right);
        shell.key(Key::Escape);
        assert!(folding(&shell));
        shell.pressed(bar(), (400.0, 20.0), Button::Right);
        shell.popup_dismissed();
        assert!(folding(&shell));
    }

    #[test]
    fn folding_back_draws_its_first_frame_at_once() {
        let mut shell = shell_on_dp1(SHIPPED);
        shell.pressed(bar(), (400.0, 20.0), Button::Right);
        play(&mut shell, 1.0);
        shell.key(Key::Escape);
        assert_eq!(shell.to_redraw(), Redraw::Surfaces(vec![On::Popup]));
    }

    #[test]
    fn folded_back_the_settings_are_gone() {
        let mut shell = shell_on_dp1(SHIPPED);
        shell.pressed(bar(), (400.0, 20.0), Button::Right);
        play(&mut shell, 1.0);
        shell.key(Key::Escape);
        play(&mut shell, 2.0);
        assert_eq!(shell.popup(), None);
    }

    #[test]
    fn folding_back_leaves_ripples_before_the_settings_are_gone() {
        let mut shell = shell_on_dp1(SHIPPED);
        shell.pressed(bar(), (400.0, 20.0), Button::Right);
        play(&mut shell, 1.0);
        shell.key(Key::Escape);
        // Some 0.15 s after the card went in: the bar's edge rises in crests
        // a pixel out of it, beside where the card was.
        let scene = play(&mut shell, 0.35);
        let edge = &scene.shapes[0];
        let crests = (0..308)
            .filter(|&x| edge.distance(Point::new(x as f32, 1.0)) < 0.0)
            .count();
        assert!(crests > 0 && crests < 308, "{crests} pixels a pixel out");
        assert!(shell.popup().is_some());
        play(&mut shell, 2.0);
        assert_eq!(shell.popup(), None);
    }

    #[test]
    fn the_settings_animate_only_while_they_move() {
        let mut shell = shell_on_dp1(SHIPPED);
        shell.pressed(bar(), (400.0, 20.0), Button::Right);
        assert!(shell.animating(On::Popup));
        play(&mut shell, 1.0);
        assert!(!shell.animating(On::Popup));
        assert!(!shell.animating(bar()));
    }

    #[test]
    fn settings_the_engine_closed_stay_closed() {
        let mut shell = shell_on_dp1(SHIPPED);
        shell.pressed(bar(), (400.0, 20.0), Button::Right);
        shell.popup_closed();
        assert_eq!(shell.popup(), None);
    }

    #[test]
    fn opened_the_card_fills_its_part_of_the_settings() {
        let mut shell = shell_on_dp1(SHIPPED);
        shell.pressed(bar(), (400.0, 20.0), Button::Right);
        let scene = play(&mut shell, 1.0);
        let [c0, c1] = settings::card(Side::Below, &Config::default(), 0.0);
        let card = &scene.shapes[0];
        let middle = Point::new((c0.x + c1.x) / 2.0, (c0.y + c1.y) / 2.0);
        assert!(card.distance(middle) < 0.0);
        assert!(card.distance(Point::new(middle.x, c1.y - 0.5)) < 0.0);
        assert!(card.distance(Point::new(middle.x, c1.y + 0.5)) > 0.0);
    }

    #[test]
    fn opening_the_card_starts_as_a_drop_narrower_than_it_is_deep() {
        let mut shell = shell_on_dp1(SHIPPED);
        shell.pressed(bar(), (400.0, 20.0), Button::Right);
        // A few frames in, about a quarter of the way.
        let scene = play(&mut shell, 0.05);
        let [p0, p1] = scene.shapes[0].bounds();
        let card = &scene.shapes[0];
        let mid_x = (p0.x + p1.x) / 2.0;
        let deep = (0..200)
            .filter(|&y| card.distance(Point::new(mid_x, y as f32)) < 0.0)
            .count();
        let wide = (0..(p1.x as i32))
            .filter(|&x| card.distance(Point::new(x as f32, 2.0)) < 0.0)
            .count();
        assert!(
            deep > 0 && wide < settings::CARD.0 as usize,
            "{deep} deep, {wide} wide"
        );
    }

    /// The bar the settings `bar = { ... }` give a monitor 1920 wide.
    fn panel_from(bar: &str) -> Panel {
        let mut shell = shell_on_dp1(&format!("return {{ bar = {{ thickness = 44, {bar} }} }}"));
        let monitor = shell.monitors.remove(&OutputId(1)).unwrap();
        monitor.screen.panels.into_iter().next().unwrap()
    }

    #[test]
    fn a_shorter_panel_sits_in_the_middle_of_its_edge_and_keeps_its_room() {
        let spec = panel_from("length = 600").surface((1920.0, 1080.0));
        assert_eq!(spec.anchor, engine::Anchor::TOP);
        assert_eq!((spec.size, spec.exclusive), ((600, 44), 44));
    }

    #[test]
    fn a_gap_is_a_margin_on_the_panels_edge() {
        let spec = panel_from("gap = 8").surface((1920.0, 1080.0));
        assert_eq!((spec.margin, spec.exclusive), ((8, 0, 0, 0), 44));
    }

    #[test]
    fn a_shorter_panel_on_its_edge_makes_room_for_its_concave_corners() {
        let hanging = panel_from("length = 600, radius = 12, inverse_radius = 12");
        assert!(hanging.hangs());
        assert_eq!(hanging.surface((1920.0, 1080.0)).size, (624, 44));
        let mut scene = Scene::new();
        hanging.draw(&mut scene, (624.0, 44.0), &Theme::builtin());
        let fillets = scene
            .shapes
            .iter()
            .filter(|s| s.sdf().is_some_and(|f| f.cut_radius > 0.0));
        assert_eq!(fillets.count(), 2);
    }

    #[test]
    fn a_panel_off_its_edge_or_spanning_it_has_no_concave_corners() {
        for text in [
            "length = 600, inverse_radius = 12, gap = 8",
            "inverse_radius = 12",
            "length = 600, radius = 12",
        ] {
            let panel = panel_from(text);
            assert!(!panel.hangs(), "{text}");
            let mut scene = Scene::new();
            panel.draw(&mut scene, (600.0, 44.0), &Theme::builtin());
            let cut = scene
                .shapes
                .iter()
                .any(|s| s.sdf().is_some_and(|f| f.cut_radius > 0.0));
            assert!(!cut, "{text}");
        }
    }

    #[test]
    fn the_radius_is_at_most_half_the_thickness() {
        assert_eq!(panel_from("radius = 100").edges.radii, [22.0; 4]);
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
    fn a_panel_in_the_middle_fits_its_components() {
        let panel = Panel {
            anchor: Anchor::Center,
            orientation: Orientation::Horizontal,
            thickness: 32.0,
            length: None,
            gap: 0.0,
            edges: Edges {
                radii: [0.0; 4],
                inverse_radius: 0.0,
                border: 0.0,
            },
            components: vec![Component::Logo, Component::Logo],
        };
        let spec = panel.surface((1920.0, 1080.0));
        // 84 is 8 + 32 + 4 + 32 + 8: both ends, two logos and the gap between them.
        assert_eq!(spec.anchor, engine::Anchor::empty());
        assert_eq!((spec.size, spec.exclusive), ((84, 32), 0));
    }

    #[test]
    fn each_monitor_gets_the_settings_its_run_returns() {
        let mut shell = Shell::new(
            r#"return {
                bar = {
                    length = monitor.width - 600,
                    orientation = if monitor.name == "HDMI-A-1" then "vertical" else "horizontal",
                },
            }"#
            .into(),
        );
        let dp1 = shell.surfaces(&output(1, "DP-1"));
        assert_eq!(
            (dp1[0].anchor, dp1[0].size),
            (engine::Anchor::TOP, (1320, 44))
        );
        let wide = Output {
            size: (2560.0, 1440.0),
            ..output(2, "HDMI-A-1")
        };
        let hdmi = shell.surfaces(&wide);
        assert_eq!(
            (hdmi[0].anchor, hdmi[0].size),
            (engine::Anchor::LEFT, (44, 1960))
        );
    }

    #[test]
    fn a_monitor_the_settings_fail_for_gets_the_defaults() {
        let mut shell = Shell::new(
            r#"if monitor.name == "HDMI-A-1" then error("not here") end
            return { bar = { thickness = 30 } }"#
                .into(),
        );
        assert_eq!(shell.surfaces(&output(1, "DP-1"))[0].size, (0, 30));
        assert_eq!(shell.surfaces(&output(2, "HDMI-A-1"))[0].size, (0, 44));
    }

    /// One file per test under the system temp dir, so that tests running in
    /// parallel never share one.
    fn settings(name: &str, text: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("tonish-{name}-{}.luau", std::process::id()));
        std::fs::write(&path, text).unwrap();
        path
    }

    /// Read from a file, with DP-1 known.
    fn shell_from(path: &Path) -> Shell {
        let mut shell = Shell::read(path.to_path_buf()).unwrap();
        shell.surfaces(&output(1, "DP-1"));
        shell
    }

    fn dp1_bar(shell: &Shell) -> &Panel {
        &shell.monitor(OutputId(1)).unwrap().screen.panels[0]
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
    fn an_edit_reshapes_the_panels() {
        let path = settings("edit", config::DEFAULT);
        let mut shell = shell_from(&path);
        let vertical = r#"return { bar = { orientation = "vertical", thickness = 30 } }"#;
        std::fs::write(&path, vertical).unwrap();
        let reshape = shell.changed(&path);
        let _ = std::fs::remove_file(&path);
        assert!(reshape);
        let panel = dp1_bar(&shell);
        assert_eq!((panel.anchor, panel.thickness), (Anchor::Left, 30.0));
    }

    #[test]
    fn an_edit_that_keeps_the_panels_size_still_draws_them_again() {
        let path = settings("radius", config::DEFAULT);
        let mut shell = shell_from(&path);
        std::fs::write(&path, "return { bar = { radius = 10 } }").unwrap();
        assert!(shell.changed(&path));
        let _ = std::fs::remove_file(&path);
        assert_eq!(shell.to_redraw(), Redraw::Everything);
    }

    #[test]
    fn a_save_that_changes_nothing_does_not_reshape() {
        let path = settings("same", config::DEFAULT);
        let mut shell = shell_from(&path);
        let reshape = shell.changed(&path);
        let _ = std::fs::remove_file(&path);
        assert!(!reshape);
    }

    #[test]
    fn a_broken_edit_keeps_the_settings_from_before() {
        let path = settings("broken", config::DEFAULT);
        let mut shell = shell_from(&path);
        std::fs::write(
            &path,
            r#"return { bar = { orientation = "vertical", thickness = 0 } }"#,
        )
        .unwrap();
        let reshape = shell.changed(&path);
        let _ = std::fs::remove_file(&path);
        assert!(!reshape);
        assert_eq!(dp1_bar(&shell).anchor, Anchor::Top);
    }

    #[test]
    fn an_edit_that_fails_for_one_monitor_applies_to_none() {
        let path = settings("one-fails", config::DEFAULT);
        let mut shell = shell_from(&path);
        shell.surfaces(&output(2, "HDMI-A-1"));
        let text = r#"if monitor.name == "HDMI-A-1" then error("not here") end
            return { bar = { thickness = 30 } }"#;
        std::fs::write(&path, text).unwrap();
        let reshape = shell.changed(&path);
        let _ = std::fs::remove_file(&path);
        assert!(!reshape);
        assert_eq!(dp1_bar(&shell).thickness, 44.0);
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
