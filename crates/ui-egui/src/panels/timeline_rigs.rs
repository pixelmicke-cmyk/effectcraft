//! UI rows for composition controls. No Layer is constructed or added to the project.
use super::*;

#[derive(Clone)]
struct Drag {
    start: f32,
    applied: i64,
    merge: String,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn draw(
    app: &mut EffectcraftApp,
    ui: &mut egui::Ui,
    comp: &Comp,
    id: u64,
    property: bool,
    r: Rect,
    cw: &Cols,
    graph_x: f32,
    tm: &TMap,
    time: Tick,
    left_clip: Rect,
    right_clip: Rect,
    actions: &mut Vec<(String, serde_json::Value)>,
) {
    let Some(rig) = comp.rigs2d.iter().find(|rig| rig.id.0 == id) else { return };
    let t = app.tokens;
    let p = ui.painter().clone();
    ui.set_clip_rect(left_clip);
    let cy = r.center().y;
    let selected = app.session.state.selected_rig2d == Some(rig.id);
    let color = t.label(rig.label);
    let left = Rect::from_min_max(r.min, pos2(graph_x - 1.0, r.max.y));
    let right = Rect::from_min_max(pos2(graph_x, r.min.y), r.max);
    p.with_clip_rect(left_clip).rect_filled(left, 0.0, if selected { t.row_selected } else { t.row_alt });
    p.with_clip_rect(right_clip).rect_filled(right, 0.0, t.tl_bg);
    if !property {
        let swatch = Rect::from_center_size(pos2(cw.label + 10.0, cy), vec2(12.0, 12.0));
        let swatch_response = cw.vis.label.then(|| {
            p.with_clip_rect(left_clip).rect_filled(swatch, 2.0, color);
            app.auto.add(&format!("timeline.rig.{id}.label"), swatch, rig.label.name());
            ui.interact(swatch, egui::Id::new(("rig-label", id)), Sense::click())
        });
        let twirl = Rect::from_center_size(pos2(cw.name + 8.0, cy), vec2(14.0, 14.0));
        let open = app.ui.timeline.open_groups.contains(&id);
        p.with_clip_rect(left_clip).text(twirl.center(), Align2::CENTER_CENTER, if open { "▾" } else { "▸" }, Tokens::ui(12.0), t.text);
        app.auto.add(&format!("timeline.rig.{id}.twirl"), twirl, "Expand IK control");
        if ui.interact(twirl, egui::Id::new(("rig-twirl", id)), Sense::click()).clicked() {
            if open {
                app.ui.timeline.open_groups.remove(&id);
            } else {
                app.ui.timeline.open_groups.insert(id);
            }
        }
        let name = Rect::from_min_max(pos2(cw.name + 22.0, r.min.y), left.max);
        let layer_name = |i| comp.layer(i).map(|l| l.name.as_str()).unwrap_or("Missing layer");
        let label = format!("IK · {} → {}", layer_name(rig.layers[0]), layer_name(rig.layers[1]));
        p.with_clip_rect(left_clip).text(name.left_center(), Align2::LEFT_CENTER, &label, Tokens::ui(12.0), color);
        app.auto.add(&format!("timeline.rig.{id}.name"), name, &label);
        let name_response = ui.interact(name, egui::Id::new(("rig-name", id)), Sense::click());
        if name_response.clicked() {
            actions.push(("rig.select2d".into(), json!({"rig":id,"timeline":true})));
        }
        let popup = egui::Id::new(("rig-label-popup", id));
        if name_response.secondary_clicked() || swatch_response.as_ref().is_some_and(|r| r.clicked() || r.secondary_clicked()) {
            widgets::open_popup(ui, popup);
        }
        if let Some(label) = widgets::label_popup(ui, popup, name.left_bottom(), rig.label, |l| app.session.prefs.label_name(l), &t) {
            actions.push(("rig.setLabel2d".into(), json!({"rig":id,"label":label.name()})));
        }
        return;
    }
    let pr = &rig.target;
    let uid = pr.uid;
    let sw = Rect::from_center_size(pos2(cw.name + 24.0, cy), vec2(16.0, 16.0));
    icons::paint(&p.with_clip_rect(left_clip), sw, Icon::Stopwatch, if pr.is_animated() { t.hot_text } else { t.text_dim });
    app.auto.add(&format!("timeline.prop.{uid}.stopwatch"), sw, "IK Position animation");
    if ui.interact(sw, egui::Id::new(("rig-stopwatch", uid)), Sense::click()).clicked() {
        actions.push(("prop.toggleAnimation".into(), json!({"rig":id,"prop":uid})));
    }
    let label = Rect::from_min_size(pos2(cw.name + 36.0, r.min.y), vec2(65.0, r.height()));
    p.with_clip_rect(left_clip).text(label.left_center(), Align2::LEFT_CENTER, "Position", Tokens::ui(12.0), t.text);
    app.auto.add(&format!("timeline.prop.{uid}.name"), label, "IK Position");
    let label_response = ui.interact(label, egui::Id::new(("rig-position", uid)), Sense::click());
    if label_response.clicked() {
        actions.push(("prop.select".into(), json!({"rig":id,"prop":uid})));
    }
    widgets::context_menu(&label_response, |ui| {
        let response = ui.add_enabled(app.session.state.value_clipboard.len() == 1, egui::Button::new("Paste Value"));
        app.auto.add(&format!("timeline.prop.{uid}.menu.pasteValue"), response.rect, "Paste Value at current time");
        if response.clicked() {
            actions.push(("keys.pasteValue".into(), json!({"rig":id,"prop":uid})));
            ui.close();
        }
    });
    let value = pr.value_at(time).as_vec2();
    let x = (cw.name + 108.0).min(graph_x - 145.0);
    for (dim, axis) in ["X", "Y"].iter().enumerate() {
        let gesture_id = egui::Id::new(("rig-position-gesture", uid, dim));
        let gesture = ui.data(|d| d.get_temp::<u64>(gesture_id).unwrap_or(0));
        let at = pos2(x + dim as f32 * 65.0, r.min.y);
        p.with_clip_rect(left_clip).text(at + vec2(0.0, r.height() / 2.0), Align2::LEFT_CENTER, *axis, Tokens::ui(11.0), t.text_dim);
        let (resp, new, finished) = widgets::hot_number_at(
            ui,
            at + vec2(13.0, 0.0),
            egui::Id::new(("rig-position-value", uid, dim)),
            value.get(dim).copied().unwrap_or(0.0),
            1.0,
            (-1e12, 1e12),
            1,
            "",
            &t,
        );
        app.auto.add(&format!("timeline.prop.{uid}.{axis}"), resp, *axis);
        if let Some(v) = new {
            if !selected {
                actions.push(("rig.select2d".into(), json!({"rig":id,"timeline":true})));
            }
            let mut next = value;
            if let Some(n) = next.get_mut(dim) {
                *n = v;
            }
            actions.push(("rig.setTarget2d".into(), json!({"rig":id,"position":next,"merge":format!("rig-position-{uid}-{dim}-{gesture}")})));
        }
        if finished {
            ui.data_mut(|d| d.insert_temp(gesture_id, gesture.saturating_add(1)));
        }
    }
    let add = Rect::from_center_size(pos2(graph_x - 12.0, cy), vec2(14.0, 14.0));
    let on = effectcraft_engine::keyframe::key_at(&pr.keys, time).is_some();
    p.with_clip_rect(left_clip).text(add.center(), Align2::CENTER_CENTER, "◆", Tokens::ui(11.0), if on { t.accent } else { t.text_dim });
    app.auto.add(&format!("timeline.prop.{uid}.addKey"), add, "Add/remove IK keyframe");
    if ui.interact(add, egui::Id::new(("rig-add-key", uid)), Sense::click()).clicked() {
        actions.push(("prop.toggleKey".into(), json!({"rig":id,"prop":uid})));
    }
    if app.ui.timeline.graph_editor {
        return;
    }
    ui.set_clip_rect(right_clip);
    let drag_id = egui::Id::new(("rig-key-drag", id));
    for k in &pr.keys {
        let center = pos2(tm.x(k.time.seconds()), cy);
        let kr = Rect::from_center_size(center, vec2(12.0, 14.0));
        let picked = app.session.state.selected_keys.iter().any(|s| s.layer.0 == id && s.prop == uid && s.time == k.time);
        let color = if picked { t.accent } else { t.text_dim };
        let icon = k.icon();
        icons::keyframe(&p.with_clip_rect(right_clip), center, 11.0, icon.left, icon.right, color);
        let resp = ui.interact(kr, egui::Id::new(("rig-key", uid, k.time.0)), Sense::click_and_drag());
        app.auto.add(&format!("timeline.rig.{id}.key.{}", k.time.0), kr, "IK Position keyframe");
        app.auto.add(&format!("timeline.keyAt.{uid}.{}", k.time.0), kr, "IK Position keyframe");
        let select = json!({"keys":[{"rig":id,"prop":uid,"time":k.time.seconds()}],"selectProperties":true});
        if resp.clicked() || resp.drag_started() {
            if !picked || resp.clicked() {
                let mut q = select.clone();
                q["toggle"] = json!(ui.input(|i| i.modifiers.shift));
                actions.push(("keys.select".into(), q));
            }
        }
        if resp.drag_started() {
            let start = ui.input(|i| i.pointer.press_origin()).unwrap_or(center).x;
            let merge = format!("rig-key-{id}-{}", ui.input(|i| i.time));
            ui.data_mut(|d| d.insert_temp(drag_id, Drag { start, applied: 0, merge }));
        }
        widgets::context_menu(&resp, |ui| {
            for (label, cmd, args) in [
                ("Copy Value", "keys.copyValue", json!({"rig":id,"prop":uid,"time":k.time.seconds()})),
                ("Paste Value", "keys.pasteValue", json!({"rig":id,"prop":uid,"time":k.time.seconds()})),
                ("Linear", "keys.interpolation", json!({"interpolation":"linear"})),
                ("Hold", "keys.interpolation", json!({"interpolation":"hold"})),
                ("Easy Ease", "keys.easyEase", json!({})),
                ("Delete Keyframe", "keys.delete", json!({})),
            ] {
                let response = ui.add_enabled(cmd != "keys.pasteValue" || app.session.state.value_clipboard.len() == 1, egui::Button::new(label));
                app.auto.add(&format!("timeline.rig.{id}.key.{}.{}", k.time.0, cmd), response.rect, label);
                if response.clicked() {
                    if !picked {
                        actions.push(("keys.select".into(), select.clone()));
                    }
                    actions.push((cmd.into(), args));
                    ui.close();
                }
            }
        });
    }
    if let Some(mut drag) = ui.data(|d| d.get_temp::<Drag>(drag_id)) {
        if ui.input(|i| i.pointer.primary_down()) {
            if let Some(pos) = ui.input(|i| i.pointer.interact_pos()) {
                let frames = ((pos.x - drag.start) as f64 / tm.pps / comp.frame_rate.frame_duration().seconds()).round();
                if frames.is_finite() && frames.abs() < 1e9 {
                    let frames = frames as i64;
                    if frames != drag.applied {
                        actions.push((
                            "keys.move".into(),
                            json!({"delta":(frames-drag.applied) as f64*comp.frame_rate.frame_duration().seconds(),"merge":drag.merge}),
                        ));
                        drag.applied = frames;
                        ui.data_mut(|d| d.insert_temp(drag_id, drag));
                    }
                }
            }
        } else {
            ui.data_mut(|d| d.remove::<Drag>(drag_id));
            app.session.history.merge_key = None;
        }
    }
}
