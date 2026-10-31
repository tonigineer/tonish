use std::collections::HashMap;

use engine::{self, App, Layer, Output, OutputId, Paint, SurfaceId, SurfaceSpec};
use renderer::{Circle, Point, Rect, Scene, Shape};

use crate::theme::Theme;

pub const EVERY: &str = "*";
pub const END: f32 = 8.0;
pub const GAP: f32 = 4.0;

pub struct Shell {
    pub screens: Vec<Screen>,
    pub theme: Theme,
    monitors: HashMap<OutputId, Option<String>>,
}

pub struct Screen {
    pub monitor: String,
    pub panels: Vec<Panel>,
}

pub struct Panel {
    pub anchor: Anchor,
    pub orientation: Orientation,
    pub thickness: f32,
    pub components: Vec<Component>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
        // todo!("write default config");

        Self {
            screens: vec![Screen {
                monitor: EVERY.into(),
                panels: vec![Panel {
                    anchor: Anchor::Top,
                    orientation: Orientation::Horizontal,
                    thickness: 44.0,
                    components: vec![Component::Logo],
                }],
            }],
            theme: Theme::builtin(),
            monitors: HashMap::new(),
        }
    }
}

impl Shell {
    pub fn screen(&self, monitor: Option<&str>) -> Option<&Screen> {
        let named = monitor.and_then(|m| self.screens.iter().find(|s| s.monitor == m));
        named.or_else(|| self.screens.iter().find(|s| s.monitor == EVERY))
    }
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
        // 8 + 32 + 4 + 32 + 8 long, floating in the middle, reserving nothing.
        assert_eq!(specs[1].anchor, engine::Anchor::empty());
        assert_eq!((specs[1].size, specs[1].exclusive), ((84, 32), 0));
        assert_eq!(
            shell.surfaces(&output(2, "DP-1")).len(),
            1,
            "every other monitor: the default"
        );
    }
}
