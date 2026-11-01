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
use tracing::{debug, trace, warn};

use super::{Engine, Role};
use crate::app::{Button, Cursor, Key, On, SurfaceId};

/// Linux's input-event-codes.h, which wl_pointer and wl_keyboard send.
const BTN_LEFT: u32 = 0x110;
const BTN_RIGHT: u32 = 0x111;
const BTN_MIDDLE: u32 = 0x112;
const KEY_ESC: u32 = 1;

pub(crate) struct Pointer {
    seat: wl_seat::WlSeat,
    wl: wl_pointer::WlPointer,
    shape: Option<WpCursorShapeDeviceV1>,
    /// The serial of the pointer's last entry into a surface, which a
    /// change of cursor has to name, and the cursor shown since.
    entered: Option<(u32, Cursor)>,
}

impl Pointer {
    /// Only when it changes: the compositor redraws the cursor for every
    /// request, and the pointer moves far more often than the cursor does.
    fn show(&mut self, cursor: Cursor) {
        let (Some(device), Some((serial, shown))) = (&self.shape, &mut self.entered) else {
            return;
        };
        if *shown == cursor {
            return;
        }
        *shown = cursor;
        device.set_shape(
            *serial,
            match cursor {
                Cursor::Default => Shape::Default,
                Cursor::Pointer => Shape::Pointer,
                Cursor::Grab => Shape::Grab,
                Cursor::Grabbing => Shape::Grabbing,
            },
        );
    }
}

impl Engine {
    /// The app's surface the pointer is on. The grab's surface takes the
    /// pointer for the popup, and lies exactly over it, so that a position
    /// on one is the same on the other.
    fn on(&self, wl: &wl_surface::WlSurface) -> Option<On> {
        let grab = self
            .grab
            .as_ref()
            .is_some_and(|g| g.popup.wl_surface() == wl);
        if grab
            || self
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
                    self.pointers.push(Pointer {
                        seat,
                        wl,
                        shape,
                        entered: None,
                    });
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
        let button = |code| match code {
            BTN_LEFT => Button::Left,
            BTN_RIGHT => Button::Right,
            BTN_MIDDLE => Button::Middle,
            other => Button::Other(other),
        };
        for event in events {
            let on = self.on(&event.surface);
            let at = (event.position.0 as f32, event.position.1 as f32);
            match event.kind {
                // The compositor leaves the cursor to the client over its
                // surfaces, and shows whatever it last had without this.
                PointerEventKind::Enter { serial } => {
                    if let Some(p) = self.pointers.iter_mut().find(|p| &p.wl == pointer) {
                        // Not what it showed elsewhere, so that the first
                        // `show` sets it.
                        p.entered = Some((serial, Cursor::Grabbing));
                        p.show(Cursor::Default);
                    }
                }
                PointerEventKind::Press {
                    button: code,
                    serial,
                    ..
                } => {
                    if let Some(data) = pointer.data::<PointerData>() {
                        self.pressed = Some((data.seat().clone(), serial));
                    }
                    let Some(on) = on else {
                        continue;
                    };
                    debug!(on = ?on, at = ?at, button = ?button(code), "pressed");
                    self.app.pressed(on, at, button(code));
                }
                PointerEventKind::Release { button: code, .. } => {
                    if let Some(on) = on {
                        trace!(on = ?on, at = ?at, button = ?button(code), "released");
                        self.app.released(on, at, button(code));
                    }
                }
                // While a button is held, the surface it was pressed on gets
                // the motion even outside it, so that a drag goes on.
                PointerEventKind::Motion { .. } => {
                    if let Some(on) = on {
                        trace!(on = ?on, at = ?at, "moved");
                        self.app.moved(on, at);
                    }
                }
                PointerEventKind::Leave { .. } => {
                    if let Some(on) = on {
                        trace!(on = ?on, "left");
                        self.app.left(on);
                    }
                }
                _ => {}
            }
            if let Some(on) = on
                && !matches!(event.kind, PointerEventKind::Leave { .. })
            {
                let cursor = self.app.cursor(on, at);
                if let Some(p) = self.pointers.iter_mut().find(|p| &p.wl == pointer) {
                    p.show(cursor);
                }
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
