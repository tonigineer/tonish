use smithay_client_toolkit::compositor::Region;
use smithay_client_toolkit::reexports::client::protocol::wl_shm;
use smithay_client_toolkit::reexports::client::{Connection, Dispatch, QueueHandle};
use smithay_client_toolkit::reexports::protocols::xdg::shell::client::xdg_positioner::{
    Anchor as PopupAnchor, Gravity,
};
use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::shell::wlr_layer::{Anchor, KeyboardInteractivity, Layer};
use smithay_client_toolkit::shell::xdg::XdgPositioner;
use smithay_client_toolkit::shell::xdg::popup::{Popup, PopupConfigure, PopupHandler};
use smithay_client_toolkit::delegate_xdg_popup;
use smithay_client_toolkit::globals::GlobalData;
use smithay_client_toolkit::reexports::client::delegate_dispatch;
use smithay_client_toolkit::reexports::protocols::xdg::decoration::zv1::client::zxdg_decoration_manager_v1::{
    self, ZxdgDecorationManagerV1,
};
use smithay_client_toolkit::reexports::protocols::xdg::shell::client::xdg_wm_base::XdgWmBase;
use smithay_client_toolkit::reexports::protocols::wp::viewporter::client::wp_viewport::{
    self, WpViewport,
};
use smithay_client_toolkit::reexports::protocols::wp::viewporter::client::wp_viewporter;
pub(super) use smithay_client_toolkit::reexports::protocols::wp::viewporter::client::wp_viewporter::WpViewporter;
use smithay_client_toolkit::shell::xdg::XdgShell;
use std::time::Instant;

use tracing::{debug, warn};

use super::{Engine, Role, Surface, Target, name};
use crate::app::PopupSpec;

impl Engine {
    /// Called after every batch of events, after the app has heard of them
    /// all, so that a popup opened for a button press opens in the same
    /// batch as the press.
    pub(super) fn update_popup(&mut self) {
        let wanted = self.app.popup();
        if wanted != self.wanted {
            self.grab_failed = false;
            self.change_popup(wanted);
        }
        self.take_grab();
    }

    /// The grab, once the popup is on the screen and has the keyboard: a
    /// grab keeps the keyboard where it is as it starts, and sway lets no
    /// surface take it while the grab lasts, so a grab taken before the
    /// popup could take the keyboard leaves Escape with some window. It
    /// still names the press the popup opened for, which wlroots and
    /// Hyprland accept a few frames late.
    fn take_grab(&mut self) {
        let Some(spec) = self.wanted.clone() else {
            return;
        };
        if !spec.grab
            || self.grab.is_some()
            || self.grab_failed
            || self.popup.is_none()
            || self.popup_waits.is_some()
        {
            return;
        }
        if let Err(e) = self.open_grab(&spec) {
            warn!("the popup cannot hear of clicks outside it: {e}");
            self.grab_failed = true;
            return;
        }
        debug!("popup grabbed");
        self.popup_input(&spec);
    }

    fn change_popup(&mut self, wanted: Option<PopupSpec>) {
        let before = std::mem::replace(&mut self.wanted, wanted.clone());
        let had_grab = self.grab.is_some();
        let Some(spec) = wanted else {
            // The app asked for this, so it is not told.
            self.grab = None;
            if self.popup.take().is_some() {
                debug!("popup closed");
            }
            return;
        };
        let moved = before.as_ref().is_none_or(|b| {
            (b.parent, b.at, b.size, b.parent_at)
                != (spec.parent, spec.at, spec.size, spec.parent_at)
        });
        let opened = self.popup.is_none() || moved;
        if opened {
            self.grab = None;
            self.popup = None;
            if let Err(e) = self.open_popup(&spec) {
                warn!("cannot open a popup: {e}");
                self.app.popup_closed();
                return;
            }
        }
        if !spec.grab {
            self.grab = None;
        }
        let keyboard_changed = before.as_ref().is_none_or(|b| b.grab != spec.grab);
        if !opened
            && keyboard_changed
            && let Some(Surface {
                role: Role::Popup { layer, .. },
                ..
            }) = &self.popup
        {
            // Applied with the commit in popup_input; the grab waits for a
            // frame after it.
            layer.set_keyboard_interactivity(keyboard(&spec));
            self.popup_waits = Some(Instant::now());
        }
        // Only when it changed: a card that comes to rest changes the spec
        // at every frame, but its input region only as it moves, and each
        // change commits two surfaces.
        let input = before.as_ref().is_none_or(|b| b.input != spec.input);
        if opened || input || keyboard_changed || had_grab != self.grab.is_some() {
            self.popup_input(&spec);
        }
        if !opened && keyboard_changed && spec.grab {
            // A frame for the grab to wait for, which a card at rest
            // wouldn't draw.
            self.draw(Target::Popup);
        }
    }

    fn open_popup(&mut self, spec: &PopupSpec) -> Result<(), String> {
        let out = self
            .outs
            .iter()
            .find(|o| o.info.id == spec.parent.0)
            .ok_or("its monitor is gone")?;
        let surface = self.compositor.create_surface(&self.qh);
        let layer = self.layer_shell.create_layer_surface(
            &self.qh,
            surface,
            Layer::Overlay,
            Some("tonish-popup"),
            Some(&out.wl),
        );
        // From the monitor's own corner, past the room other surfaces keep.
        layer.set_anchor(Anchor::TOP | Anchor::LEFT);
        layer.set_exclusive_zone(-1);
        layer.set_margin(spec.at.1, 0, 0, spec.at.0);
        layer.set_size(spec.size.0, spec.size.1);
        layer.set_keyboard_interactivity(keyboard(spec));
        layer.commit();
        self.popup_waits = Some(Instant::now());
        debug!(
            monitor = %name(&out.info),
            at = ?spec.at,
            size = ?spec.size,
            "popup opened"
        );
        self.popup = Some(Surface {
            window: None,
            role: Role::Popup {
                layer,
                spec: spec.clone(),
            },
            size: (0, 0),
            configured: false,
            frame_pending: false,
            drawn_at: None,
            timer: None,
        });
        Ok(())
    }

    /// A transparent xdg_popup exactly over the popup, which holds the
    /// compositor's grab, which a click outside ends. The popup the app
    /// draws can't hold it, because a layer
    /// surface can't take a grab. It takes the pointer for the popup too,
    /// where the popup does, since Hyprland lets only the grab's own surface
    /// have it during the grab, a layer surface's popup having no parent
    /// there. As large as the popup, so that a drag whose input is the whole
    /// popup never leaves it: sway ends a drag that leaves the grab's
    /// surface, holding on to no surface during a grab.
    fn open_grab(&mut self, spec: &PopupSpec) -> Result<(), String> {
        let (w, h) = (spec.size.0 as i32, spec.size.1 as i32);
        let (seat, serial) = self
            .pressed
            .clone()
            .ok_or("no button press to take it from")?;
        let xdg = self
            .xdg_shell
            .as_ref()
            .ok_or("the compositor has no xdg-shell")?;
        let (output, si) = (spec.parent.0, spec.parent.1.0);
        let out = self
            .outs
            .iter()
            .find(|o| o.info.id == output)
            .ok_or("its monitor is gone")?;
        let Some(Role::Layer { layer, .. }) = out.surfaces.get(si).map(|s| &s.role) else {
            return Err("the surface it hangs from is gone".into());
        };
        // Exactly there, without sliding or flipping it, so that the
        // pointer's place on it maps back onto the popup.
        let positioner = XdgPositioner::new(xdg).map_err(|e| e.to_string())?;
        positioner.set_size(w.max(1), h.max(1));
        positioner.set_anchor_rect(
            spec.at.0 - spec.parent_at.0,
            spec.at.1 - spec.parent_at.1,
            1,
            1,
        );
        positioner.set_anchor(PopupAnchor::TopLeft);
        positioner.set_gravity(Gravity::BottomRight);
        let surface = self.compositor.create_surface(&self.qh);
        let popup = Popup::from_surface(None, &positioner, &self.qh, surface, xdg)
            .map_err(|e| e.to_string())?;
        layer.get_popup(popup.xdg_popup());
        let viewport = self
            .viewporter
            .as_ref()
            .map(|v| v.get_viewport(popup.wl_surface(), &self.qh, ()));
        // Before the first commit, which the protocol requires of a grab.
        popup.xdg_popup().grab(&seat, serial);
        popup.wl_surface().commit();
        self.grab = Some(Grab {
            viewport,
            popup,
            size: (w.max(1), h.max(1)),
        });
        Ok(())
    }

    /// The part of the popup that takes the pointer goes to the grab's
    /// surface while there is one, and the popup's own lets every click
    /// through to it; an empty region lets them through to what is under it.
    pub(super) fn popup_input(&mut self, spec: &PopupSpec) {
        let Some(surf) = &self.popup else {
            return;
        };
        let (Ok(part), Ok(none)) = (Region::new(&self.compositor), Region::new(&self.compositor))
        else {
            return;
        };
        if let Some((x, y, w, h)) = spec.input {
            part.add(x, y, w, h);
        }
        let region = match &self.grab {
            Some(g) => {
                let ws = g.popup.wl_surface();
                ws.set_input_region(Some(part.wl_region()));
                ws.commit();
                &none
            }
            None => &part,
        };
        let ws = surf.role.wl_surface();
        ws.set_input_region(Some(region.wl_region()));
        match &surf.window {
            Some(window) => window.commit(ws.clone()),
            None => ws.commit(),
        }
    }

    fn is_grab(&self, popup: &Popup) -> bool {
        self.grab.as_ref().is_some_and(|g| &g.popup == popup)
    }
}

/// The popup's own layer surface takes the keyboard while it holds the
/// grab. The grab doesn't give it: it keeps keys going to the surface that
/// had them, and on sway that is never the bar, a layer surface that takes
/// no keyboard, so Escape would reach some window instead.
fn keyboard(spec: &PopupSpec) -> KeyboardInteractivity {
    if spec.grab {
        KeyboardInteractivity::Exclusive
    } else {
        KeyboardInteractivity::None
    }
}

/// The grab's surface, and its size in logical pixels, the popup's.
pub(super) struct Grab {
    /// Before `popup`, so that it goes first, before its surface.
    viewport: Option<WpViewport>,
    pub(super) popup: Popup,
    size: (i32, i32),
}

impl Drop for Grab {
    fn drop(&mut self) {
        if let Some(v) = &self.viewport {
            v.destroy();
        }
    }
}

impl PopupHandler for Engine {
    /// Transparent, in a buffer of shared memory: it never changes, and the
    /// GPU need not know of it.
    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        popup: &Popup,
        _: PopupConfigure,
    ) {
        let Some(g) = self.grab.as_ref().filter(|g| &g.popup == popup) else {
            return;
        };
        // One pixel stretched over it, where the compositor can stretch it,
        // rather than a buffer as large as the monitor.
        let (w, h) = g.size;
        let (bw, bh) = match &g.viewport {
            Some(v) => {
                v.set_destination(w, h);
                (1, 1)
            }
            None => (w, h),
        };
        let Ok((buffer, px)) = self
            .pool
            .create_buffer(bw, bh, bw * 4, wl_shm::Format::Argb8888)
        else {
            return;
        };
        px.fill(0);
        let ws = popup.wl_surface();
        if buffer.attach_to(ws).is_ok() {
            ws.damage_buffer(0, 0, bw, bh);
        }
        ws.commit();
    }

    /// The compositor ends the grab on a click outside the app's surfaces.
    fn done(&mut self, _: &Connection, _: &QueueHandle<Self>, popup: &Popup) {
        if self.is_grab(popup) {
            self.grab = None;
            debug!("popup dismissed");
            self.app.popup_dismissed();
            // The popup takes the pointer itself again.
            if let Some(spec) = self.wanted.clone() {
                self.popup_input(&spec);
            }
        }
    }
}

impl Engine {
    /// The popup's own surface is a layer surface, configured like the
    /// others; it is drawn once the compositor gives it a size.
    pub(super) fn configure_popup(&mut self, size: (u32, u32)) {
        let Some(surf) = self.popup.as_mut() else {
            return;
        };
        if surf.configured && surf.size == size {
            return;
        }
        surf.size = size;
        surf.configured = true;
        self.draw(Target::Popup);
    }
}

// By hand rather than with sctk's delegate_xdg_shell, whose dispatch for
// the decoration manager asks for a window handler, though tonish makes no
// windows.
delegate_dispatch!(Engine: [XdgWmBase: GlobalData] => XdgShell);
delegate_xdg_popup!(Engine);

impl Dispatch<WpViewporter, ()> for Engine {
    fn event(
        _: &mut Self,
        _: &WpViewporter,
        _: wp_viewporter::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WpViewport, ()> for Engine {
    fn event(
        _: &mut Self,
        _: &WpViewport,
        _: wp_viewport::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

/// `XdgShell::bind` binds the decoration manager too, which has no events.
impl Dispatch<ZxdgDecorationManagerV1, GlobalData> for Engine {
    fn event(
        _: &mut Self,
        _: &ZxdgDecorationManagerV1,
        _: zxdg_decoration_manager_v1::Event,
        _: &GlobalData,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
