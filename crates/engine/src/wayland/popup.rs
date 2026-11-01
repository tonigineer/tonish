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
use smithay_client_toolkit::shell::xdg::XdgShell;
use tracing::{debug, warn};

use super::{Engine, Role, Surface, Target, name};
use crate::app::PopupSpec;

impl Engine {
    /// Called after every batch of events, after the app has heard of them
    /// all, so that a popup opened for a button press opens in the same
    /// batch and its grab can still name that press.
    pub(super) fn update_popup(&mut self) {
        let wanted = self.app.popup();
        if wanted == self.wanted {
            return;
        }
        let before = std::mem::replace(&mut self.wanted, wanted.clone());
        let Some(spec) = wanted else {
            // The app asked for this, so it is not told.
            self.grab = None;
            if self.popup.take().is_some() {
                debug!("popup closed");
            }
            return;
        };
        let moved =
            before.is_none_or(|b| (b.parent, b.at, b.size) != (spec.parent, spec.at, spec.size));
        if self.popup.is_none() || moved {
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
        } else if self.grab.is_none()
            && let Err(e) = self.open_grab(&spec)
        {
            warn!("the popup cannot take the keyboard or hear of clicks outside it: {e}");
        }
        self.popup_input(&spec);
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
        layer.set_keyboard_interactivity(KeyboardInteractivity::None);
        layer.commit();
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

    /// An xdg_popup of a single transparent pixel on the parent, which only
    /// holds the compositor's grab: the keyboard, and the end of the grab on
    /// a click outside the app's surfaces. The popup the app draws can't
    /// hold it, because a layer surface can't take a grab.
    fn open_grab(&mut self, spec: &PopupSpec) -> Result<(), String> {
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
        let positioner = XdgPositioner::new(xdg).map_err(|e| e.to_string())?;
        positioner.set_size(1, 1);
        positioner.set_anchor_rect(0, 0, 1, 1);
        positioner.set_anchor(PopupAnchor::TopLeft);
        positioner.set_gravity(Gravity::BottomRight);
        let surface = self.compositor.create_surface(&self.qh);
        let popup = Popup::from_surface(None, &positioner, &self.qh, surface, xdg)
            .map_err(|e| e.to_string())?;
        layer.get_popup(popup.xdg_popup());
        let empty = Region::new(&self.compositor).map_err(|e| e.to_string())?;
        popup.wl_surface().set_input_region(Some(empty.wl_region()));
        // Before the first commit, which the protocol requires of a grab.
        popup.xdg_popup().grab(&seat, serial);
        popup.wl_surface().commit();
        self.grab = Some(popup);
        Ok(())
    }

    /// Committed with the popup's next frame, which the app draws whenever
    /// its answer changes; an empty region lets every click through.
    fn popup_input(&mut self, spec: &PopupSpec) {
        let Some(surf) = &self.popup else {
            return;
        };
        let Ok(region) = Region::new(&self.compositor) else {
            return;
        };
        if let Some((x, y, w, h)) = spec.input {
            region.add(x, y, w, h);
        }
        let ws = surf.role.wl_surface();
        ws.set_input_region(Some(region.wl_region()));
        match &surf.window {
            Some(window) => window.commit(ws.clone()),
            None => ws.commit(),
        }
    }

    fn is_grab(&self, popup: &Popup) -> bool {
        self.grab.as_ref() == Some(popup)
    }
}

impl PopupHandler for Engine {
    /// The grab's pixel, transparent, in a buffer of shared memory: it never
    /// changes, and the GPU need not know of it.
    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        popup: &Popup,
        _: PopupConfigure,
    ) {
        if !self.is_grab(popup) {
            return;
        }
        let Ok((buffer, px)) = self.pool.create_buffer(1, 1, 4, wl_shm::Format::Argb8888) else {
            return;
        };
        px.fill(0);
        let ws = popup.wl_surface();
        if buffer.attach_to(ws).is_ok() {
            ws.damage_buffer(0, 0, 1, 1);
        }
        ws.commit();
    }

    /// The compositor ends the grab on a click outside the app's surfaces.
    fn done(&mut self, _: &Connection, _: &QueueHandle<Self>, popup: &Popup) {
        if self.is_grab(popup) {
            self.grab = None;
            debug!("popup dismissed");
            self.app.popup_dismissed();
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
