use renderer::Scene;
use smithay_client_toolkit::shell::wlr_layer::Anchor;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct OutputId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SurfaceId(pub usize);

#[derive(Clone, Debug)]
pub struct Output {
    pub id: OutputId,
    pub name: Option<String>,
    pub size: (f32, f32),
    pub scale: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Layer {
    Background,
    Bottom,
    Top,
    Overlay,
}

#[derive(Clone, Debug)]
pub struct SurfaceSpec {
    pub name: &'static str,
    pub layer: Layer,
    pub anchor: Anchor,
    pub size: (u32, u32),
    pub exclusive: i32,
    pub margin: (i32, i32, i32, i32),
}

pub struct Paint<'a> {
    pub scene: &'a mut Scene,
    pub size: (f32, f32),
    pub scale: f32,
}

pub trait App {
    fn surfaces(&mut self, output: &Output) -> Vec<SurfaceSpec>;

    fn draw(&mut self, output: OutputId, surface: SurfaceId, paint: &mut Paint);
}
