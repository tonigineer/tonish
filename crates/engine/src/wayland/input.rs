use smithay_client_toolkit::reexports::client::{
    Connection, Dispatch, Proxy, QueueHandle, WEnum,
    protocol::{wl_keyboard, wl_pointer, wl_seat, wl_surface},
};
use smithay_client_toolkit::reexports::protocols::wp::cursor_shape::v1::client::wp_cursor_shape_device_v1::{
    Shape, WpCursorShapeDeviceV1,
};
use smithay_client_toolkit::seat::pointer::{
    PointerData, PointerEvent, PointerEventKind, PointerHandler,
};
use smithay_client_toolkit::seat::{Capability, SeatHandler, SeatState};
use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::{delegate_pointer, delegate_seat};
use tracing::{debug, warn};

use super::{Engine, Role};
use crate::app::{Button, Key, On, SurfaceId};

/// Linux's input-event-codes.h, which wl_pointer and wl_keyboard send.
const BTN_LEFT: u32 = 0x110;
const BTN_RIGHT: u32 = 0x111;
const BTN_MIDDLE: u32 = 0x112;
const KEY_ESC: u32 = 1;

pub(crate) struct Pointer {
    seat: wl_seat::WlSeat,
    wl: wl_pointer::WlPointer,
    shape: Option<WpCursorShapeDeviceV1>,
}

impl Engine {
    fn on(&self, wl: &wl_surface::WlSurface) -> Option<On> {
        if self
            .popup
            .as_ref()
            .is_some_and(|p| p.role.wl_surface() == wl)
        {
            return Some(On::Popup);
        }
        self.outs.iter().find_map(|o| {
            let si = o.surfaces.iter().position(|s| match &s.role {
                Role::Layer { layer, .. } => layer.wl_surface() == wl,
                Role::Popup { .. } => false,
            })?;
            Some(On::Surface(o.info.id, SurfaceId(si)))
        })
    }
}

impl SeatHandler for Engine {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }

    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}

    fn new_capability(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        match capability {
            Capability::Pointer => match self.seat_state.get_pointer(qh, &seat) {
                Ok(wl) => {
                    let shape = self
                        .cursor_shapes
                        .as_ref()
                        .map(|m| m.get_shape_device(&wl, qh));
                    self.pointers.push(Pointer { seat, wl, shape });
                    debug!("pointer added");
                }
                Err(e) => warn!("cannot use the pointer, so clicks do nothing: {e}"),
            },
            // Raw, without sctk's keyboard, which needs xkbcommon.
            Capability::Keyboard => {
                let keyboard = seat.get_keyboard(qh, ());
                self.keyboards.push((seat, keyboard));
                debug!("keyboard added");
            }
            _ => {}
        }
    }

    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        match capability {
            Capability::Pointer => self.pointers.retain(|p| {
                if p.seat != seat {
                    return true;
                }
                if let Some(shape) = &p.shape {
                    shape.destroy();
                }
                p.wl.release();
                false
            }),
            Capability::Keyboard => self.keyboards.retain(|(s, keyboard)| {
                if *s != seat {
                    return true;
                }
                keyboard.release();
                false
            }),
            _ => {}
        }
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl PointerHandler for Engine {
    fn pointer_frame(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        pointer: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        for event in events {
            match event.kind {
                // The compositor leaves the cursor to the client over its
                // surfaces, and shows whatever it last had without this.
                PointerEventKind::Enter { serial } => {
                    let shape = self.pointers.iter().find(|p| &p.wl == pointer);
                    if let Some(shape) = shape.and_then(|p| p.shape.as_ref()) {
                        shape.set_shape(serial, Shape::Default);
                    }
                }
                PointerEventKind::Press { button, serial, .. } => {
                    if let Some(data) = pointer.data::<PointerData>() {
                        self.pressed = Some((data.seat().clone(), serial));
                    }
                    let Some(on) = self.on(&event.surface) else {
                        continue;
                    };
                    let button = match button {
                        BTN_LEFT => Button::Left,
                        BTN_RIGHT => Button::Right,
                        BTN_MIDDLE => Button::Middle,
                        other => Button::Other(other),
                    };
                    let at = (event.position.0 as f32, event.position.1 as f32);
                    debug!(on = ?on, at = ?at, button = ?button, "pressed");
                    self.app.pressed(on, at, button);
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<wl_keyboard::WlKeyboard, ()> for Engine {
    fn event(
        engine: &mut Self,
        _: &wl_keyboard::WlKeyboard,
        event: wl_keyboard::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        // The keymap's descriptor closes as the event drops, unread.
        if let wl_keyboard::Event::Key {
            key: KEY_ESC,
            state: WEnum::Value(wl_keyboard::KeyState::Pressed),
            ..
        } = event
        {
            debug!(key = ?Key::Escape, "key");
            engine.app.key(Key::Escape);
        }
    }
}

delegate_seat!(Engine);
delegate_pointer!(Engine);
