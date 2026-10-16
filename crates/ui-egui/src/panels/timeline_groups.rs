//! Organization rows; never passed to layer hit testing, trimming or render code.
use super::*;

pub(super) fn rows(rows: Vec<Row>, comp: &Comp) -> Vec<Row> {
    if comp.timeline_groups.is_empty() {
        return rows;
    }
    let mut out = vec![];
    let mut seen = std::collections::BTreeSet::new();
    for row in &rows {
        if let Some(group) = comp.timeline_groups.iter().find(|g| g.layers.contains(&row.layer)) {
            if seen.insert(group.id.0) {
                out.push(Row { layer: LayerId(0), depth: 0, kind: RowKind::TimelineGroup { id: group.id.0 } });
                if !group.collapsed {
                    out.extend(rows.iter().filter(|r| group.layers.contains(&r.layer)).cloned());
                }
            }
        } else {
            out.push(row.clone());
        }
    }
    // Keep empty groups accessible so they can be renamed, refilled or dissolved.
    for group in &comp.timeline_groups {
        if seen.insert(group.id.0) {
            out.push(Row { layer: LayerId(0), depth: 0, kind: RowKind::TimelineGroup { id: group.id.0 } });
        }
    }
    out
}

pub(super) fn layer_menu(
    ui: &mut egui::Ui,
    auto: &mut crate::automation::Registry,
    comp: &Comp,
    layers: &[u64],
    actions: &mut Vec<(String, serde_json::Value)>,
) {
    let response = ui.button("Create Group");
    auto.add("timeline.group.create", response.rect, "Create Group");
    if response.clicked() {
        actions.push(("timeline.group.create".into(), json!({"layers":layers})));
        ui.close();
    }
    if !comp.timeline_groups.is_empty() {
        let submenu = ui.menu_button("Add to Group", |ui| {
            for group in &comp.timeline_groups {
                let response = ui.button(&group.name);
                auto.add(&format!("timeline.group.{}.add", group.id.0), response.rect, &group.name);
                if response.clicked() {
                    actions.push(("timeline.group.add".into(), json!({"group":group.id.0,"layers":layers})));
                    ui.close();
                }
            }
        });
        auto.add("timeline.group.addMenu", submenu.response.rect, "Add to Group");
        let response = ui.button("Remove from Group");
        auto.add("timeline.group.remove", response.rect, "Remove from Group");
        if response.clicked() {
            actions.push(("timeline.group.remove".into(), json!({"layers":layers})));
            ui.close();
        }
    }
    ui.separator();
}

#[allow(clippy::too_many_arguments)]
pub(super) fn draw(
    app: &mut EffectcraftApp,
    ui: &mut egui::Ui,
    comp: &Comp,
    id: u64,
    r: Rect,
    cw: &Cols,
    graph_x: f32,
    actions: &mut Vec<(String, serde_json::Value)>,
) {
    let Some(group) = comp.timeline_groups.iter().find(|g| g.id.0 == id) else { return };
    let t = app.tokens;
    let color = t.label(group.label);
    ui.painter().rect_filled(r, 0.0, t.row_alt);
    let swatch = Rect::from_center_size(pos2(cw.label + 10.0, r.center().y), vec2(12.0, 12.0));
    if cw.vis.label {
        ui.painter().rect_filled(swatch, 2.0, color);
        app.auto.add(&format!("timeline.group.{id}.label"), swatch, group.label.name());
    }
    let twirl = Rect::from_center_size(pos2(cw.name + 8.0, r.center().y), vec2(14.0, 14.0));
    ui.painter().text(twirl.center(), Align2::CENTER_CENTER, if group.collapsed { "▸" } else { "▾" }, Tokens::ui(12.0), color);
    let arrow = ui.interact(twirl, egui::Id::new(("timeline-group-twirl", id)), Sense::click());
    app.auto.add(&format!("timeline.group.{id}.twirl"), twirl, "Expand/collapse group");
    if arrow.clicked() {
        actions.push(("timeline.group.collapse".into(), json!({"group":id,"collapsed":!group.collapsed})));
    }
    let name = Rect::from_min_max(pos2(cw.name + 22.0, r.min.y), pos2(graph_x - 2.0, r.max.y));
    let edit_id = egui::Id::new(("timeline-group-rename", id));
    let response = if let Some((mut text, mut focused)) = ui.data(|d| d.get_temp::<(String, bool)>(edit_id)) {
        let response = ui.put(name, egui::TextEdit::singleline(&mut text).id(edit_id.with("input")));
        app.auto.add(&format!("timeline.group.{id}.renameInput"), response.rect, "Group name");
        if !focused {
            response.request_focus();
            focused = true;
        }
        let escape = ui.input(|i| i.key_pressed(egui::Key::Escape));
        if escape || response.lost_focus() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            if !escape {
                actions.push(("timeline.group.rename".into(), json!({"group":id,"name":text})));
            }
            ui.data_mut(|d| d.remove::<(String, bool)>(edit_id));
        } else {
            ui.data_mut(|d| d.insert_temp(edit_id, (text, focused)));
        }
        response
    } else {
        ui.painter().text(name.left_center(), Align2::LEFT_CENTER, &group.name, Tokens::ui(12.0), color);
        let response = ui.interact(name, egui::Id::new(("timeline-group-name", id)), Sense::click());
        if response.clicked() {
            actions.push(("layer.select".into(), json!({"layers":group.layers.iter().map(|l|l.0).collect::<Vec<_>>()})));
        }
        if response.double_clicked() {
            ui.data_mut(|d| d.insert_temp(edit_id, (group.name.clone(), false)));
        }
        response
    };
    app.auto.add(&format!("timeline.group.{id}.name"), name, &group.name);
    widgets::context_menu(&response, |ui| {
        let rename = ui.button("Rename Group");
        app.auto.add(&format!("timeline.group.{id}.rename"), rename.rect, "Rename Group");
        if rename.clicked() {
            ui.data_mut(|d| d.insert_temp(edit_id, (group.name.clone(), false)));
            ui.close();
        }
        let submenu = ui.menu_button("Label", |ui| {
            for label in effectcraft_engine::color::Label::ALL {
                let response = widgets::label_entry(ui, label, &app.session.prefs.label_name(label), label == group.label, &t);
                app.auto.add(&format!("timeline.group.{id}.color.{label:?}"), response.rect, label.name());
                if response.clicked() {
                    actions.push(("timeline.group.label".into(), json!({"group":id,"label":label.name()})));
                    ui.close();
                }
            }
        });
        app.auto.add(&format!("timeline.group.{id}.labelMenu"), submenu.response.rect, "Group label");
        let add = ui.add_enabled(!app.session.state.selected_layers.is_empty(), egui::Button::new("Add Selected Layers"));
        app.auto.add(&format!("timeline.group.{id}.addSelected"), add.rect, "Add Selected Layers");
        if add.clicked() {
            actions.push(("timeline.group.add".into(), json!({"group":id})));
            ui.close();
        }
        let remove = ui.add_enabled(!app.session.state.selected_layers.is_empty(), egui::Button::new("Remove Selected Layers"));
        app.auto.add(&format!("timeline.group.{id}.removeSelected"), remove.rect, "Remove Selected Layers");
        if remove.clicked() {
            actions.push(("timeline.group.remove".into(), json!({"group":id})));
            ui.close();
        }
        let dissolve = ui.button("Dissolve Group");
        app.auto.add(&format!("timeline.group.{id}.dissolve"), dissolve.rect, "Dissolve Group");
        if dissolve.clicked() {
            actions.push(("timeline.group.dissolve".into(), json!({"group":id})));
            ui.close();
        }
    });
}
