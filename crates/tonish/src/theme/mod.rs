use renderer::Rgba;

#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    pub primary: Rgba,
    pub surface: Rgba,
    pub on_surface: Rgba,
    pub outline: Rgba,
}

impl Theme {
    pub fn builtin() -> Self {
        Self {
            primary: Rgba::from_hex(0xc9a96e, 1.0),
            surface: Rgba::from_hex(0x111318, 1.0),
            on_surface: Rgba::from_hex(0xe8e3d8, 1.0),
            outline: Rgba::from_hex(0x3a3e48, 1.0),
        }
    }
}
