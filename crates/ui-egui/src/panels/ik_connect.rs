//! Composition-local, non-document draft for the Connect workflow.
use crate::EffectcraftApp;
use effectcraft_engine::{geom::vec3, render::EvalCtx};
use egui::{Rect, pos2, vec2};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Draft {
    pub comp: u64,
    pub layers: Vec<u64>,
    pub root: Option<u64>,
    pub endpoint: Option<[f64; 2]>,
    pub picking: bool,
    pub reverse_bend: bool,
    pub error: String,
}

pub fn sync(app: &mut EffectcraftApp) {
    let comp = app.session.active_comp_id().map(|c| c.0).unwrap_or(0);
    let mut layers: Vec<_> = app.session.state.selected_layers.iter().map(|l| l.0).collect();
    layers.sort_unstable();
    if app.ui.ik_connect.comp != comp || app.ui.ik_connect.layers != layers {
        app.ui.ik_connect = Draft { comp, layers, ..Draft::default() };
    }
    let d = &mut app.ui.ik_connect;
    if let (Some(c), [a, b]) = (app.session.active_comp(), d.layers.as_slice()) {
        let a = c.layer(effectcraft_engine::project::LayerId(*a));
        let b = c.layer(effectcraft_engine::project::LayerId(*b));
        if let (Some(a), Some(b)) = (a, b) {
            let inferred = match (a.parent == Some(b.id), b.parent == Some(a.id)) {
                (true, false) => Some(b.id.0),
                (false, true) => Some(a.id.0),
                _ => None,
            };
            if let Some(root) = inferred
                && d.root != Some(root)
            {
                d.root = Some(root);
                d.endpoint = None;
                d.picking = false;
            }
        }
    }
}

pub fn tip(app: &EffectcraftApp) -> Option<effectcraft_engine::project::LayerId> {
    let d = &app.ui.ik_connect;
    let root = d.root?;
    if d.layers.len() != 2 || !d.layers.contains(&root) {
        return None;
    }
    d.layers.iter().find(|id| **id != root).map(|id| effectcraft_engine::project::LayerId(*id))
}

pub fn choose_endpoint(app: &mut EffectcraftApp, point: [f64; 2]) -> Result<(), String> {
    let id = tip(app).ok_or("Choose the root layer first.")?;
    let c = app.session.active_comp().ok_or("Composition not found.")?;
    let cid = app.session.active_comp_id().ok_or("Composition not found.")?;
    let layer = c.layer(id).ok_or("The forearm layer no longer exists.")?;
    if layer.is_3d() {
        return Err("Only 2D layers are supported.".into());
    }
    let ctx = EvalCtx { expr: app.session.expr.as_deref(), ..EvalCtx::new(&app.session.project, cid, c, app.session.time()) };
    let inv = ctx.world_matrix(layer).inverse().ok_or("The forearm transform cannot be inverted.")?;
    let local = inv.apply(vec3(point[0], point[1], 0.0));
    if !local.x.is_finite() || !local.y.is_finite() {
        return Err("The endpoint is outside the supported coordinate range.".into());
    }
    app.ui.ik_connect.endpoint = Some([local.x, local.y]);
    app.ui.ik_connect.picking = false;
    app.ui.ik_connect.error.clear();
    Ok(())
}

fn button(app: &mut EffectcraftApp, ui: &mut egui::Ui, x: f32, y: &mut f32, w: f32, id: &str, label: &str, enabled: bool) -> bool {
    let rect = Rect::from_min_size(pos2(x, *y), vec2(w, 24.0));
    let response = ui.add_enabled_ui(enabled, |ui| ui.put(rect, egui::Button::new(label))).inner;
    app.auto.add(id, rect, label);
    *y += 28.0;
    enabled && response.clicked()
}

pub fn show(app: &mut EffectcraftApp, ui: &mut egui::Ui, x: f32, mut y: f32, w: f32) -> f32 {
    sync(app);
    let Some(c) = app.session.active_comp_arc() else { return y };
    let ids = app.ui.ik_connect.layers.clone();
    if ids.len() != 2 {
        return y;
    }
    let inferred = c.layers.iter().filter(|l| ids.contains(&l.id.0) && l.parent.is_some_and(|p| p != l.id && ids.contains(&p.0))).count() == 1;
    if !inferred {
        for id in &ids {
            if let Some(layer) = c.layer(effectcraft_engine::project::LayerId(*id)) {
                let label = format!("{} Root: {}", if app.ui.ik_connect.root == Some(*id) { "●" } else { "○" }, layer.name);
                if button(app, ui, x, &mut y, w, &format!("properties.ik2d.root.{id}"), &label, true) {
                    if app.ui.ik_connect.root != Some(*id) {
                        app.ui.ik_connect.endpoint = None;
                    }
                    app.ui.ik_connect.root = Some(*id);
                    app.ui.ik_connect.picking = false;
                }
            }
        }
    } else if let Some(root) = app.ui.ik_connect.root.and_then(|id| c.layer(effectcraft_engine::project::LayerId(id))) {
        ui.put(Rect::from_min_size(pos2(x, y), vec2(w, 22.0)), egui::Label::new(format!("Root: {} (Parent & Link)", root.name)));
        y += 26.0;
    }
    let label = if app.ui.ik_connect.picking {
        "Click free end · Esc cancels"
    } else if app.ui.ik_connect.endpoint.is_some() {
        "Change endpoint…"
    } else {
        "Choose endpoint in Composition…"
    };
    if button(app, ui, x, &mut y, w, "properties.ik2d.endpoint", label, app.ui.ik_connect.root.is_some()) {
        app.ui.ik_connect.picking = !app.ui.ik_connect.picking;
    }
    let label = if app.ui.ik_connect.reverse_bend { "Bend: clockwise" } else { "Bend: counterclockwise" };
    if button(app, ui, x, &mut y, w, "properties.ik2d.bend", label, true) {
        app.ui.ik_connect.reverse_bend = !app.ui.ik_connect.reverse_bend;
    }
    let ready = app.ui.ik_connect.endpoint.is_some() && !app.ui.ik_connect.picking;
    if button(app, ui, x, &mut y, w, "properties.ik2d.connect", "Connect", ready) {
        let d = app.ui.ik_connect.clone();
        match app.session.execute(
            "rig.connect2d",
            json!({"comp":d.comp,"layers":d.layers,"root":d.root,"end_point":d.endpoint,"bend_direction":if d.reverse_bend {-1} else {1}}),
        ) {
            Ok(reply) => {
                if let Err(e) = app.session.execute("rig.select2d", json!({"rig":reply.get("rig")})) {
                    app.ui.status = e.to_string();
                }
                app.ui.ik_connect.endpoint = None;
                app.ui.ik_connect.error.clear();
            }
            Err(e) => app.ui.ik_connect.error = format!("Cannot connect: {e}"),
        }
    }
    if !app.ui.ik_connect.error.is_empty() {
        let response = ui.put(Rect::from_min_size(pos2(x, y), vec2(w, 60.0)), egui::Label::new(&app.ui.ik_connect.error).wrap());
        app.auto.add("properties.ik2d.error", response.rect, &app.ui.ik_connect.error);
        y += response.rect.height() + 8.0;
    }
    y + 10.0
}
