use renderer::{Circle, Point, Rect, Scene, Shape};

use crate::theme::Theme;

pub const EVERY: &str = "*";
pub const END: f32 = 8.0;
pub const GAP: f32 = 4.0;

pub struct Shell {
    pub screens: Vec<Screen>,
    pub theme: Theme,
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
