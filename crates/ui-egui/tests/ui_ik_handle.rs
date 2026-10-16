//! Exercise real pointer gestures against the Composition viewer (no helper layers).
use effectcraft_engine::{
    Session,
    geom::vec3,
    render::{EvalCtx, RenderOpts},
};
use effectcraft_ui_egui::{EffectcraftApp, panels::viewer};
use egui::{Event, Modifiers, PointerButton, Pos2, pos2, vec2};
use egui_kittest::Harness;
use serde_json::json;

#[test]
fn rig_label_menu_uses_layer_palette_and_paints_matching_track_and_handle() {
    use effectcraft_engine::color::Label;
    use egui_kittest::kittest::Queryable as _;
    let (original, rig) = harness(true);
    let mut s = Session::default();
    s.project = original.state().session.project.clone();
    s.state = original.state().session.state.clone();
    let green = s.prefs.labels.iter_mut().find(|l| l.name == "Green").unwrap();
    green.name = "My IK Green".into();
    green.color = "#23a67b".into();
    let cid = s.active_comp_id().unwrap();
    let pixels = s.render(cid, s.time(), RenderOpts::default()).data;
    let mut h = Harness::builder().with_size(vec2(1400.0, 1000.0)).build_eframe(|_| EffectcraftApp::new(s));
    h.run_steps(5);
    let entry = h.state().auto.find(&format!("timeline.rig.{rig}.name")).unwrap().rect;
    let pos = pos2(entry[0] + entry[2] / 2.0, entry[1] + entry[3] / 2.0);
    for pressed in [true, false] {
        h.input_mut().events.push(Event::PointerMoved(pos));
        h.input_mut().events.push(Event::PointerButton { pos, button: PointerButton::Secondary, pressed, modifiers: Modifiers::NONE });
        h.step();
    }
    h.run_steps(3);
    for label in Label::ALL {
        let name = h.state().session.prefs.label_name(label);
        assert!(h.query_by_label(&name).is_some(), "missing shared palette entry {name}");
    }
    h.get_by_label("My IK Green").click();
    h.run_steps(4);
    assert_eq!(h.state().session.active_comp().unwrap().rigs2d[0].label, Label::Green);
    let color = h.state().tokens.label(Label::Green);
    assert_eq!(color, egui::Color32::from_rgb(0x23, 0xa6, 0x7b));
    let handle = center(&h, rig);
    assert!(h.output().shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Circle(circle) if circle.center.distance(handle)<0.1 && circle.stroke.color==color && (circle.radius-8.0).abs()<0.1)));
    let rect = h.state().auto.find(&format!("timeline.rig.{rig}.label")).unwrap().rect;
    let swatch = pos2(rect[0] + rect[2] / 2.0, rect[1] + rect[3] / 2.0);
    assert!(h.output().shapes.iter().any(|shape| matches!(&shape.shape,egui::Shape::Rect(r) if r.rect.center().distance(swatch)<0.1 && r.fill==color)));
    assert_eq!(pixels, h.state().session.render(cid, h.state().session.time(), RenderOpts::default()).data);
    assert!(h.state_mut().session.undo());
    h.run_steps(3);
    assert_eq!(h.state().session.active_comp().unwrap().rigs2d[0].label, Label::Blue);
    assert!(h.state_mut().session.redo());
    h.run_steps(3);
    assert_eq!(h.state().session.active_comp().unwrap().rigs2d[0].label, Label::Green);
}

fn harness(rigged: bool) -> (Harness<'static, EffectcraftApp>, u64) {
    let mut s = Session::default();
    s.execute("comp.new", json!({"width":256,"height":128,"duration":2,"frameRate":24})).unwrap();
    for (name, width, position) in [("Upper", 50, [40, 64, 0]), ("Lower", 40, [90, 64, 0])] {
        s.execute("layer.newSolid", json!({"name":name,"width":width,"height":8})).unwrap();
        s.execute("prop.set", json!({"layer":name,"path":"transform/position","value":position})).unwrap();
    }
    let id = if rigged {
        s.execute("rig.connect2d", json!({"layers":["Upper","Lower"],"root":"Upper","end_point":[40,4]})).unwrap()["rig"].as_u64().unwrap()
    } else {
        0
    };
    s.mark_saved();
    let mut h = Harness::builder().with_size(vec2(1000.0, 600.0)).build_ui_state(
        |ui, app: &mut EffectcraftApp| {
            app.auto.begin_frame();
            viewer::show(app, ui, ui.max_rect());
        },
        EffectcraftApp::new(s),
    );
    h.run_steps(4);
    (h, id)
}
fn center(h: &Harness<'_, EffectcraftApp>, rig: u64) -> Pos2 {
    let e = h.state().auto.find(&format!("viewer.ikHandle.{rig}")).unwrap();
    pos2(e.rect[0] + e.rect[2] / 2.0, e.rect[1] + e.rect[3] / 2.0)
}
fn pointer(h: &mut Harness<'_, EffectcraftApp>, p: Pos2, pressed: bool) {
    h.input_mut().events.push(Event::PointerMoved(p));
    h.input_mut().events.push(Event::PointerButton { pos: p, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
    h.step();
}
fn click(h: &mut Harness<'_, EffectcraftApp>, p: Pos2) {
    pointer(h, p, true);
    pointer(h, p, false);
    h.run_steps(2);
}

#[test]
fn ik_key_context_menu_copies_value_and_pastes_at_key_or_current_time() {
    let (original, rig) = harness(true);
    let mut s = Session::default();
    s.project = original.state().session.project.clone();
    s.state = original.state().session.state.clone();
    let uid = s.active_comp().unwrap().rigs2d[0].target.uid;
    for (time, value) in [(0, json!([90, 40])), (1, json!([100, 50]))] {
        s.execute("prop.addKey", json!({"rig":rig,"prop":uid,"time":time,"value":value})).unwrap();
    }
    let mut h = Harness::builder().with_size(vec2(1400.0, 1000.0)).build_eframe(|_| EffectcraftApp::new(s));
    h.state_mut().ui.timeline.pps = Some(120.0);
    h.run_steps(5);
    click_id(&mut h, &format!("timeline.rig.{rig}.twirl"));
    let right_click = |h: &mut Harness<'_, EffectcraftApp>, id: &str| {
        let rect = h.state().auto.find(id).unwrap().rect;
        let pos = pos2(rect[0] + rect[2] / 2.0, rect[1] + rect[3] / 2.0);
        for pressed in [true, false] {
            h.input_mut().events.push(Event::PointerMoved(pos));
            h.input_mut().events.push(Event::PointerButton { pos, button: PointerButton::Secondary, pressed, modifiers: Modifiers::NONE });
            h.step();
        }
        h.run_steps(3);
    };
    right_click(&mut h, &format!("timeline.rig.{rig}.key.0"));
    click_id(&mut h, &format!("timeline.rig.{rig}.key.0.keys.copyValue"));
    let tick = effectcraft_engine::time::Tick::from_seconds_f64(1.0).0;
    right_click(&mut h, &format!("timeline.rig.{rig}.key.{tick}"));
    click_id(&mut h, &format!("timeline.rig.{rig}.key.{tick}.keys.pasteValue"));
    let target = &h.state().session.active_comp().unwrap().rigs2d[0].target;
    assert_eq!(target.keys[1].time.0, tick);
    assert_eq!(target.keys[1].value, target.keys[0].value);
    assert!(h.state_mut().session.undo());
    assert_eq!(h.state().session.active_comp().unwrap().rigs2d[0].target.keys[1].value.as_vec2(), [100.0, 50.0]);
    h.state_mut().session.set_time(effectcraft_engine::time::Tick::from_seconds_f64(1.5));
    h.run_steps(3);
    right_click(&mut h, &format!("timeline.prop.{uid}.name"));
    click_id(&mut h, &format!("timeline.prop.{uid}.menu.pasteValue"));
    let target = &h.state().session.active_comp().unwrap().rigs2d[0].target;
    assert_eq!(target.keys.len(), 3);
    assert_eq!(target.keys[2].time, h.state().session.time());
    assert_eq!(target.keys[2].value.as_vec2(), [90.0, 40.0]);
}
fn drag(h: &mut Harness<'_, EffectcraftApp>, rig: u64, from: Pos2, to: Pos2, expected: [f64; 2]) {
    pointer(h, from, true);
    for i in 1..=6 {
        h.input_mut().events.push(Event::PointerMoved(from + (to - from) * (i as f32 / 6.0)));
        h.step();
        let s = &h.state().session;
        let c = s.active_comp().unwrap();
        let ctx = EvalCtx::new(&s.project, s.active_comp_id().unwrap(), c, s.time());
        let r = c.rigs2d.iter().find(|r| r.id.0 == rig).unwrap();
        let p = r.target.value_at(s.time()).as_vec2();
        let tip = ctx.world_matrix(c.layer(r.layers[1]).unwrap()).apply(vec3(40.0, 4.0, 0.0));
        assert!((tip.x - p[0]).hypot(tip.y - p[1]) < 1e-5, "arm must update before release");
    }
    pointer(h, to, false);
    h.run_steps(3);
    let actual = h.state().session.active_comp().unwrap().rigs2d[0].target.value_at(h.state().session.time()).as_vec2();
    assert!((actual[0] - expected[0]).abs() < 1e-3 && (actual[1] - expected[1]).abs() < 1e-3, "{actual:?} != {expected:?}");
}

#[test]
fn handle_click_preserves_layer_selection_and_never_enters_rendered_pixels() {
    let (mut h, rig) = harness(true);
    let cid = h.state().session.active_comp_id().unwrap();
    let before = h.state().session.project.clone();
    let selection = h.state().session.state.selected_layers.clone();
    let undo = h.state().session.history.undo.len();
    let pixels = h.state().session.render(cid, h.state().session.time(), RenderOpts::default()).data;
    let p = center(&h, rig);
    click(&mut h, p);
    assert_eq!(h.state().session.state.selected_rig2d.unwrap().0, rig);
    assert_eq!(h.state().session.state.selected_layers, selection);
    assert_eq!(h.state().session.project, before);
    assert_eq!(h.state().session.history.undo.len(), undo);
    assert!(!h.state().session.is_dirty());
    assert_eq!(pixels, h.state().session.render(cid, h.state().session.time(), RenderOpts::default()).data);
    let point = viewer::comp_to_screen(&h.ctx, [35.0, 64.0]).unwrap();
    click(&mut h, point);
    assert!(h.state().session.state.selected_rig2d.is_none());
    assert_eq!(h.state().session.state.selected_layers, vec![before.comp(cid).unwrap().rigs2d[0].layers[0]]);
}

#[test]
fn drag_respects_zoom_pan_and_grab_offset_and_is_one_undo_step() {
    for zoom in [0.8, 2.5] {
        let (mut h, rig) = harness(true);
        h.state_mut().ui.viewer.zoom = Some(zoom);
        h.state_mut().ui.viewer.pan = [30.0, -20.0];
        h.run_steps(3);
        let before = h.state().session.project.clone();
        let cid = h.state().session.active_comp_id().unwrap();
        let undo = h.state().session.history.undo.len();
        let selection = h.state().session.state.selected_layers.clone();
        let from = center(&h, rig) + vec2(5.0, 2.0);
        let to = from + vec2(-20.0 * zoom, -24.0 * zoom);
        drag(&mut h, rig, from, to, [90.0, 40.0]);
        let after = h.state().session.project.clone();
        assert_eq!(h.state().session.history.undo.len(), undo + 1);
        assert_eq!(after.comp(cid).unwrap().layers, before.comp(cid).unwrap().layers);
        assert_eq!(after.comp(cid).unwrap().layers.len(), 2);
        assert_eq!(h.state().session.state.selected_layers, selection);
        assert!(h.state_mut().session.undo());
        assert_eq!(h.state().session.project, before);
        assert!(!h.state().session.is_dirty());
        assert!(h.state_mut().session.redo());
        assert_eq!(h.state().session.project, after);
        h.run_steps(3);
        let from = center(&h, rig);
        let to = from + vec2(5.0 * zoom, 5.0 * zoom);
        drag(&mut h, rig, from, to, [95.0, 45.0]);
        assert_eq!(h.state().session.history.undo.len(), undo + 2);
        assert!(h.state_mut().session.undo());
        assert_eq!(h.state().session.project, after);
    }
}

#[test]
fn projects_without_rigs_keep_normal_viewer_selection() {
    let (mut h, _) = harness(false);
    assert!(!h.state().auto.elements.iter().any(|e| e.id.starts_with("viewer.ikHandle.")));
    let before = h.state().session.project.clone();
    let p = viewer::comp_to_screen(&h.ctx, [35.0, 64.0]).unwrap();
    click(&mut h, p);
    assert_eq!(h.state().session.project, before);
    assert_eq!(h.state().session.active_comp().unwrap().layers.len(), 2);
    assert_eq!(h.state().session.state.selected_layers.len(), 1);
}

#[test]
fn full_app_rejected_ik_drag_reports_reflection_without_editing_project() {
    let (original, _) = harness(false);
    let mut s = Session::default();
    s.project = original.state().session.project.clone();
    s.state = original.state().session.state.clone();
    s.execute("prop.set", json!({"layer":"Lower","path":"transform/scale","value":[-63.3,61.5,100]})).unwrap();
    let rig = s.execute("rig.connect2d", json!({"layers":["Upper","Lower"],"root":"Upper","end_point":[40,4]})).unwrap()["rig"].as_u64().unwrap();
    s.execute("layer.select", json!({"layers":["Upper","Lower"]})).unwrap();
    s.mark_saved();
    let mut h = Harness::builder().with_size(vec2(1280.0, 900.0)).build_eframe(|_| EffectcraftApp::new(s));
    h.run_steps(5);
    let before = h.state().session.project.clone();
    let selected = h.state().session.state.selected_layers.clone();
    let undo = h.state().session.history.undo.len();
    assert!(h.state().auto.find("viewer.ikError").is_none());
    let from = center(&h, rig);
    pointer(&mut h, from, true);
    h.input_mut().events.push(Event::PointerMoved(from + vec2(-25.0, -30.0)));
    h.step();
    h.run_steps(3);
    // The gesture was claimed by IK and the target command reached validation.
    assert_eq!(h.state().session.state.selected_rig2d.unwrap().0, rig);
    assert!(h.state().ui.status.contains("Scale X/Y"));
    assert!(h.state().ui.status.contains("Lower"));
    assert!(h.state().auto.find("viewer.ikError").is_some());
    pointer(&mut h, from + vec2(-25.0, -30.0), false);
    h.run_steps(4);
    assert!(h.state().auto.find("viewer.ikError").is_some(), "error must survive releasing the pointer");
    let error = h.state().auto.find("viewer.ikError").unwrap();
    assert!(error.label.contains("Lower") && error.label.contains("Scale X/Y") && error.label.contains("reflection"));
    assert_eq!(h.state().session.project, before);
    assert_eq!(h.state().session.state.selected_layers, selected);
    assert_eq!(h.state().session.history.undo.len(), undo);
    assert!(!h.state().session.is_dirty());
    // A subsequent document change invalidates the stale diagnostic.
    h.state_mut().session.execute("prop.set", json!({"layer":"Lower","path":"transform/opacity","value":90})).unwrap();
    h.run_steps(3);
    assert!(h.state().auto.find("viewer.ikError").is_none());
}

#[test]
fn full_app_drags_supported_ik_with_both_bones_selected() {
    let (original, _) = harness(false);
    let mut s = Session::default();
    s.project = original.state().session.project.clone();
    s.state = original.state().session.state.clone();
    s.execute("prop.set", json!({"layer":"Lower","path":"transform/scale","value":[63.3,61.5,100]})).unwrap();
    let rig = s.execute("rig.connect2d", json!({"layers":["Upper","Lower"],"root":"Upper","end_point":[40,4]})).unwrap()["rig"].as_u64().unwrap();
    s.execute("layer.select", json!({"layers":["Upper","Lower"]})).unwrap();
    let mut h = Harness::builder().with_size(vec2(1280.0, 900.0)).build_eframe(|_| EffectcraftApp::new(s));
    h.run_steps(5);
    let before = h.state().session.project.clone();
    let selected = h.state().session.state.selected_layers.clone();
    let undo = h.state().session.history.undo.len();
    let from = center(&h, rig);
    let to = viewer::comp_to_screen(&h.ctx, [90.0, 40.0]).unwrap();
    drag(&mut h, rig, from, to, [90.0, 40.0]);
    let s = &h.state().session;
    assert_eq!(s.active_comp().unwrap().layers, before.comp(s.active_comp_id().unwrap()).unwrap().layers);
    assert_eq!(s.state.selected_layers, selected);
    assert_eq!(s.history.undo.len(), undo + 1);
    assert!(h.state_mut().session.undo());
    assert_eq!(h.state().session.project, before);
    assert!(h.state_mut().session.redo());
}
fn connect_harness(parented: bool) -> Harness<'static, EffectcraftApp> {
    let (h, _) = harness(false);
    let mut s = Session::default();
    s.project = h.state().session.project.clone();
    s.state = h.state().session.state.clone();
    if parented {
        s.execute("layer.setParent", json!({"layers":["Lower"],"parent":"Upper"})).unwrap();
        s.execute("prop.set", json!({"layer":"Upper","path":"transform/rotation","value":25})).unwrap();
    }
    s.execute("layer.select", json!({"layers":["Lower","Upper"]})).unwrap();
    s.mark_saved();
    let mut h = Harness::builder().with_size(vec2(1000.0, 700.0)).build_ui_state(
        |ui, app: &mut EffectcraftApp| {
            let initialized = egui::Id::new("connect-test-fonts");
            if !ui.data(|d| d.get_temp::<bool>(initialized).unwrap_or(false)) {
                effectcraft_ui_egui::theme::install(ui.ctx(), &app.tokens, "en");
                ui.data_mut(|d| d.insert_temp(initialized, true));
                return;
            }
            app.auto.begin_frame();
            let r = ui.max_rect();
            viewer::show(app, ui, egui::Rect::from_min_max(r.min, pos2(650.0, r.max.y)));
            effectcraft_ui_egui::panels::properties::show(app, ui, egui::Rect::from_min_max(pos2(660.0, r.min.y), r.max));
        },
        EffectcraftApp::new(s),
    );
    h.run_steps(4);
    h
}
fn click_id(h: &mut Harness<'_, EffectcraftApp>, id: &str) {
    let r = h.state().auto.find(id).unwrap().rect;
    click(h, pos2(r[0] + r[2] / 2.0, r[1] + r[3] / 2.0));
}

#[test]
fn connect_ui_picks_local_endpoint_without_changing_selection_or_pose() {
    for parented in [false, true] {
        let mut h = connect_harness(parented);
        let before = h.state().session.project.clone();
        let selected = h.state().session.state.selected_layers.clone();
        let upper = h.state().session.active_comp().unwrap().layers.iter().find(|l| l.name == "Upper").unwrap().id;
        if !parented {
            click_id(&mut h, &format!("properties.ik2d.root.{}", upper.0));
        }
        assert_eq!(h.state().ui.ik_connect.root, Some(upper.0));
        h.state_mut().ui.viewer.zoom = Some(2.0);
        h.state_mut().ui.viewer.pan = [25.0, -20.0];
        h.run_steps(3);
        let undo = h.state().session.history.undo.len();
        click_id(&mut h, "properties.ik2d.endpoint");
        let s = &h.state().session;
        let c = s.active_comp().unwrap();
        let lower = c.layers.iter().find(|l| l.name == "Lower").unwrap();
        let ctx = EvalCtx::new(&s.project, s.active_comp_id().unwrap(), c, s.time());
        let endpoint = ctx.world_matrix(lower).apply(vec3(40.0, 4.0, 0.0));
        let point = viewer::comp_to_screen(&h.ctx, [endpoint.x as f32, endpoint.y as f32]).unwrap();
        click(&mut h, point);
        assert_eq!(h.state().session.project, before);
        assert_eq!(h.state().session.state.selected_layers, selected);
        assert!(h.state().auto.find("viewer.ikEndpoint").is_some());
        let local = h.state().ui.ik_connect.endpoint.unwrap();
        assert!((local[0] - 40.0).abs() < 1e-4 && (local[1] - 4.0).abs() < 1e-4);
        click_id(&mut h, "properties.ik2d.bend");
        let s = &h.state().session;
        let c = s.active_comp().unwrap();
        let ctx = EvalCtx::new(&s.project, s.active_comp_id().unwrap(), c, s.time());
        let matrices: Vec<_> = c.layers.iter().map(|l| ctx.world_matrix(l)).collect();
        click_id(&mut h, "properties.ik2d.connect");
        let s = &h.state().session;
        let c = s.active_comp().unwrap();
        assert_eq!(c.layers.len(), 2);
        assert_eq!(c.rigs2d.len(), 1);
        let ctx = EvalCtx::new(&s.project, s.active_comp_id().unwrap(), c, s.time());
        for (l, before) in c.layers.iter().zip(matrices) {
            let after = ctx.world_matrix(l);
            for (a, b) in after.0.iter().flatten().zip(before.0.iter().flatten()) {
                assert!((a - b).abs() < 1e-6);
            }
        }
        assert!(h.state().auto.find(&format!("viewer.ikHandle.{}", c.rigs2d[0].id.0)).is_some());
        assert_eq!(s.history.undo.len(), undo + 1);
        assert!(h.state_mut().session.undo());
        assert_eq!(h.state().session.project, before);
        assert!(h.state_mut().session.redo());
    }
}

#[test]
fn endpoint_mode_cancels_and_connect_errors_are_atomic() {
    let mut h = connect_harness(false);
    let id = h.state().session.state.selected_layers[0];
    click_id(&mut h, &format!("properties.ik2d.root.{}", id.0));
    click_id(&mut h, "properties.ik2d.endpoint");
    h.input_mut().events.push(Event::Key { key: egui::Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    h.step();
    assert!(!h.state().ui.ik_connect.picking);
    h.input_mut().events.push(Event::Key { key: egui::Key::Escape, physical_key: None, pressed: false, repeat: false, modifiers: Modifiers::NONE });
    h.step();
    click_id(&mut h, "properties.ik2d.endpoint");
    assert!(h.state().ui.ik_connect.picking);
    let p = viewer::comp_to_screen(&h.ctx, [110.0, 64.0]).unwrap();
    click(&mut h, p);
    assert!(h.state().ui.ik_connect.endpoint.is_some());
    h.state_mut().session.execute("layer.setSwitch", json!({"layers":[id.0],"switch":"locked","value":true})).unwrap();
    let before = h.state().session.project.clone();
    let undo = h.state().session.history.undo.len();
    click_id(&mut h, "properties.ik2d.connect");
    assert!(!h.state().ui.ik_connect.error.is_empty());
    assert_eq!(h.state().session.project, before);
    assert_eq!(h.state().session.history.undo.len(), undo);
    h.state_mut().session.execute("layer.select", json!({"layers":[id.0]})).unwrap();
    h.run_steps(3);
    assert!(h.state().ui.ik_connect.endpoint.is_none());
}

#[test]
fn timeline_control_selects_animates_scrubs_and_moves_existing_property_keys() {
    let (original, rig) = harness(true);
    let mut s = Session::default();
    s.project = original.state().session.project.clone();
    s.state = original.state().session.state.clone();
    let uid = s.active_comp().unwrap().rigs2d[0].target.uid;
    let mut h = Harness::builder().with_size(vec2(1400.0, 1000.0)).build_eframe(|_| EffectcraftApp::new(s));
    h.state_mut().ui.timeline.pps = Some(120.0);
    h.run_steps(5);
    let before = h.state().session.project.clone();
    click_id(&mut h, &format!("timeline.rig.{rig}.name"));
    assert_eq!(h.state().session.state.selected_rig2d.unwrap().0, rig);
    assert!(h.state().session.state.selected_layers.is_empty());
    assert_eq!(h.state().session.project, before);
    // No A/V, effects or Parent & Link widgets exist for a control owner.
    assert!(h.state().auto.find(&format!("timeline.layer.{rig}.name")).is_none());
    click_id(&mut h, &format!("timeline.rig.{rig}.twirl"));
    assert!(h.state().auto.find(&format!("timeline.prop.{uid}.X")).is_some());
    assert!(h.state().auto.find(&format!("timeline.prop.{uid}.Y")).is_some());
    click_id(&mut h, &format!("timeline.prop.{uid}.stopwatch"));
    h.state_mut().session.set_time(effectcraft_engine::time::Tick::from_seconds_f64(1.0));
    h.run_steps(3);
    let from = center(&h, rig);
    let to = viewer::comp_to_screen(&h.ctx, [90.0, 40.0]).unwrap();
    drag(&mut h, rig, from, to, [90.0, 40.0]);
    let c = h.state().session.active_comp().unwrap();
    assert_eq!(c.layers.len(), 2);
    assert_eq!(c.rigs2d[0].target.uid, uid);
    assert_eq!(c.rigs2d[0].target.keys.len(), 2);
    let second = effectcraft_engine::time::Tick::from_seconds_f64(1.0);
    let id = format!("timeline.rig.{rig}.key.{}", second.0);
    let key_rect = h.state().auto.find(&id).unwrap().rect;
    let key_center = pos2(key_rect[0] + key_rect[2] / 2.0, key_rect[1] + key_rect[3] / 2.0);
    assert!(
        h.output().shapes.iter().any(|s| {
            matches!(&s.shape,egui::Shape::Path(path) if path.fill!=egui::Color32::TRANSPARENT && path.points.iter().any(|p|p.distance(key_center)<6.0))
                && s.clip_rect.contains(key_center)
        }),
        "the key must be painted in the graph column, not only have a hit target"
    );
    click_id(&mut h, &id);
    assert_eq!(h.state().session.state.selected_keys.len(), 1);
    let r = h.state().auto.find(&id).unwrap().rect;
    let from = pos2(r[0] + r[2] / 2.0, r[1] + r[3] / 2.0);
    let before_move = h.state().session.project.clone();
    pointer(&mut h, from, true);
    for i in 1..=4 {
        h.input_mut().events.push(Event::PointerMoved(from + vec2(i as f32 * 15.0, 0.0)));
        h.step();
    }
    pointer(&mut h, from + vec2(60.0, 0.0), false);
    h.run_steps(3);
    assert_eq!(h.state().session.active_comp().unwrap().rigs2d[0].target.keys[1].time, effectcraft_engine::time::Tick::from_seconds_f64(1.5));
    assert!(h.state_mut().session.undo());
    assert_eq!(h.state().session.project, before_move);
    assert!(h.state_mut().session.redo());
    h.state_mut().session.set_time(effectcraft_engine::time::Tick::from_seconds_f64(0.75));
    h.run_steps(3);
    let c = h.state().session.active_comp().unwrap();
    assert_eq!(c.rigs2d[0].target.value_at(h.state().session.time()).as_vec2(), [100.0, 52.0]);
    h.state_mut().session.execute("edit.clear", json!({})).unwrap();
    assert_eq!(h.state().session.active_comp().unwrap().rigs2d[0].target.keys.len(), 1);
    assert_eq!(h.state().session.active_comp().unwrap().layers.len(), 2);
}
