use smithay_client_toolkit::reexports::client::{Connection, Dispatch, QueueHandle};
use smithay_client_toolkit::reexports::protocols::xdg::shell::client::xdg_positioner::{
    Anchor, ConstraintAdjustment, Gravity,
};
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
use crate::app::{PopupSpec, Side};

impl Engine {
    /// Called after every batch of events, after the app has heard of them
    /// all, so that a popup opened for a button press opens in the same
    /// batch and its grab can still name that press.
    pub(super) fn update_popup(&mut self) {
        let wanted = self.app.popup();
        if wanted == self.wanted {
            return;
        }
        self.wanted.clone_from(&wanted);
        // The app asked for this, so it is not told.
        if self.popup.take().is_some() {
            debug!("popup closed");
        }
        if let Some(spec) = wanted
            && let Err(e) = self.open_popup(spec)
        {
            warn!("cannot open a popup: {e}");
            self.app.popup_closed();
        }
    }

    fn open_popup(&mut self, spec: PopupSpec) -> Result<(), String> {
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
        positioner.set_size(spec.size.0 as i32, spec.size.1 as i32);
        let (x, y, w, h) = spec.anchor;
        positioner.set_anchor_rect(x, y, w, h);
        let (anchor, gravity) = match spec.side {
            Side::Above => (Anchor::TopLeft, Gravity::TopRight),
            Side::Below => (Anchor::BottomLeft, Gravity::BottomRight),
            Side::Left => (Anchor::TopLeft, Gravity::BottomLeft),
            Side::Right => (Anchor::TopRight, Gravity::BottomRight),
        };
        positioner.set_anchor(anchor);
        positioner.set_gravity(gravity);
        let g = spec.gap;
        let (dx, dy) = match spec.side {
            Side::Above => (0, -g),
            Side::Below => (0, g),
            Side::Left => (-g, 0),
            Side::Right => (g, 0),
        };
        positioner.set_offset(dx, dy);
        // Slid or flipped rather than cut off at the monitor's edge.
        positioner.set_constraint_adjustment(
            ConstraintAdjustment::SlideX
                | ConstraintAdjustment::SlideY
                | ConstraintAdjustment::FlipX
                | ConstraintAdjustment::FlipY,
        );
        let surface = self.compositor.create_surface(&self.qh);
        let popup = Popup::from_surface(None, &positioner, &self.qh, surface, xdg)
            .map_err(|e| e.to_string())?;
        layer.get_popup(popup.xdg_popup());
        // Before the first commit, which the protocol requires of a grab.
        if let Some((seat, serial)) = &self.pressed {
            popup.xdg_popup().grab(seat, *serial);
        }
        popup.wl_surface().commit();
        debug!(
            monitor = %name(&out.info),
            anchor = ?spec.anchor,
            side = ?spec.side,
            size = ?spec.size,
            "popup opened"
        );
        self.popup = Some(Surface {
            window: None,
            role: Role::Popup { popup, spec },
            size: (0, 0),
            configured: false,
        });
        Ok(())
    }

    fn is_popup(&self, popup: &Popup) -> bool {
        self.popup
            .as_ref()
            .is_some_and(|s| matches!(&s.role, Role::Popup { popup: p, .. } if p == popup))
    }
}

impl PopupHandler for Engine {
    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        popup: &Popup,
        config: PopupConfigure,
    ) {
        if !self.is_popup(popup) {
            return;
        }
        let Some(surf) = self.popup.as_mut() else {
            return;
        };
        let size = (config.width.max(0) as u32, config.height.max(0) as u32);
        if surf.configured && surf.size == size {
            return;
        }
        surf.size = size;
        surf.configured = true;
        self.draw(Target::Popup);
    }

    /// The compositor closes it on a click outside the app's surfaces.
    fn done(&mut self, _: &Connection, _: &QueueHandle<Self>, popup: &Popup) {
        if self.is_popup(popup) {
            self.lose_popup();
        }
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
