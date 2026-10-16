use effectcraft_engine::{Session, geom::vec3, render::EvalCtx, time::Tick};
use effectcraft_ui_egui::EffectcraftApp;
use egui::{Event, Modifiers, PointerButton, pos2, vec2};
use egui_kittest::Harness;
use serde_json::json;

fn click(h: &mut Harness<'_, EffectcraftApp>, id: &str, button: PointerButton) {
    let r = h.state().auto.find(id).unwrap_or_else(|| panic!("missing {id}")).rect;
    let pos = if let Some(prefix) = id.strip_suffix(".row") {
        // The row automation rect includes switch cells; click inside its name column.
        let twirl = h.state().auto.find(&format!("{prefix}.twirl")).unwrap().rect;
        pos2(twirl[0] + twirl[2] / 2.0 + 55.0, twirl[1] + twirl[3] / 2.0)
    } else {
        pos2(r[0] + r[2] / 2.0, r[1] + r[3] / 2.0)
    };
    for pressed in [true, false] {
        h.input_mut().events.push(Event::PointerMoved(pos));
        h.input_mut().events.push(Event::PointerButton { pos, button, pressed, modifiers: Modifiers::NONE });
        h.step();
    }
    h.run_steps(3);
}

#[test]
fn independent_groups_hide_tracks_preserve_ik_and_allow_cross_group_layer_selection() {
    let mut s = Session::default();
    s.execute("comp.new", json!({"width":256,"height":128,"duration":3,"frameRate":24})).unwrap();
    for (name, pos) in [("Upper", [40, 64, 0]), ("Lower", [90, 64, 0]), ("Other", [10, 10, 0]), ("Fourth", [20, 10, 0])] {
        s.execute("layer.newSolid", json!({"name":name,"width":40,"height":8})).unwrap();
        s.execute("prop.set", json!({"layer":name,"path":"transform/position","value":pos})).unwrap();
    }
    let rig = s.execute("rig.connect2d", json!({"layers":["Upper","Lower"],"root":"Upper","end_point":[40,4]})).unwrap()["rig"].as_u64().unwrap();
    s.execute("prop.addKey", json!({"rig":rig,"path":"position","time":0,"value":[100,60]})).unwrap();
    s.execute("prop.addKey", json!({"rig":rig,"path":"position","time":1,"value":[85,40]})).unwrap();
    let upper = s.active_comp().unwrap().layers.iter().find(|l| l.name == "Upper").unwrap().id.0;
    let other = s.active_comp().unwrap().layers.iter().find(|l| l.name == "Other").unwrap().id.0;
    s.execute("layer.select", json!({"layers":["Upper","Lower"]})).unwrap();
    let mut h = Harness::builder().with_size(vec2(1500.0, 1100.0)).build_eframe(|_| EffectcraftApp::new(s));
    h.run_steps(5);
    click(&mut h, &format!("timeline.layer.{upper}.row"), PointerButton::Secondary);
    click(&mut h, "timeline.group.create", PointerButton::Primary);
    let first = h.state().session.active_comp().unwrap().timeline_groups[0].id.0;
    let second = h.state_mut().session.execute("timeline.group.create", json!({"layers":["Other","Fourth"],"name":"Body"})).unwrap()["group"].as_u64().unwrap();
    h.run_steps(3);
    click(&mut h, &format!("timeline.group.{first}.twirl"), PointerButton::Primary);
    assert!(h.state().auto.find(&format!("timeline.layer.{upper}.row")).is_none());
    assert!(h.state().auto.find(&format!("timeline.rig.{rig}.name")).is_none());
    assert!(h.state().auto.find(&format!("viewer.ikHandle.{rig}")).is_some());
    assert!(h.state().auto.find(&format!("timeline.layer.{other}.row")).is_some());
    click(&mut h, &format!("timeline.group.{second}.twirl"), PointerButton::Primary);
    assert!(h.state().auto.find(&format!("timeline.layer.{other}.row")).is_none());
    click(&mut h, &format!("timeline.group.{first}.twirl"), PointerButton::Primary);
    assert!(h.state().auto.find(&format!("timeline.layer.{upper}.row")).is_some());
    assert!(h.state().auto.find(&format!("timeline.layer.{other}.row")).is_none());
    assert!(h.state_mut().session.undo());
    h.run_steps(3);
    assert!(h.state().auto.find(&format!("timeline.layer.{upper}.row")).is_none());
    assert!(h.state_mut().session.redo());
    h.run_steps(3);
    click(&mut h, &format!("timeline.group.{second}.twirl"), PointerButton::Primary);
    click(&mut h, &format!("timeline.layer.{upper}.row"), PointerButton::Primary);
    assert_eq!(h.state().session.state.selected_layers.len(), 1);
    click(&mut h, &format!("timeline.layer.{other}.row"), PointerButton::Primary);
    assert_eq!(h.state().session.state.selected_layers.iter().next().unwrap().0, other);
    h.state_mut().session.execute("layer.select", json!({"layers":[upper,other]})).unwrap();
    h.state_mut().session.execute("prop.set", json!({"layer":upper,"path":"transform/opacity","value":65,"selected":true})).unwrap();
    assert_eq!(h.state().session.state.selected_layers.len(), 2);
    for id in [upper, other] {
        let layer = h.state().session.active_comp().unwrap().layers.iter().find(|l| l.id.0 == id).unwrap();
        assert_eq!(layer.props.prop("transform/opacity").unwrap().value.as_f64(), 65.0);
    }
    h.state_mut().session.execute("timeline.group.collapse", json!({"group":first,"collapsed":true})).unwrap();
    h.state_mut().session.set_time(Tick::from_seconds_f64(0.5));
    h.run_steps(3);
    let s = &h.state().session;
    let comp = s.active_comp().unwrap();
    let r = &comp.rigs2d[0];
    let ctx = EvalCtx::new(&s.project, s.active_comp_id().unwrap(), comp, s.time());
    let tip = ctx.world_matrix(comp.layer(r.layers[1]).unwrap()).apply(vec3(40.0, 4.0, 0.0));
    let target = r.target.value_at(s.time()).as_vec2();
    assert!((tip.x - target[0]).hypot(tip.y - target[1]) < 1e-6);
    // Rename and dissolve through the actual group menu, leaving contents present.
    click(&mut h, &format!("timeline.group.{first}.name"), PointerButton::Secondary);
    click(&mut h, &format!("timeline.group.{first}.rename"), PointerButton::Primary);
    assert!(h.state().auto.find(&format!("timeline.group.{first}.renameInput")).is_some());
    h.key_press_modifiers(Modifiers::COMMAND, egui::Key::A);
    h.event(Event::Text("Left Arm".into()));
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
    assert_eq!(h.state().session.active_comp().unwrap().timeline_groups[0].name, "Left Arm");
    click(&mut h, &format!("timeline.group.{first}.name"), PointerButton::Secondary);
    click(&mut h, &format!("timeline.group.{first}.rename"), PointerButton::Primary);
    h.input_mut().events.push(Event::Key { key: egui::Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    h.step();
    h.run_steps(3);
    click(&mut h, &format!("timeline.group.{first}.name"), PointerButton::Secondary);
    click(&mut h, &format!("timeline.group.{first}.labelMenu"), PointerButton::Primary);
    h.run_steps(3);
    click(&mut h, &format!("timeline.group.{first}.color.Fuchsia"), PointerButton::Primary);
    assert_eq!(h.state().session.active_comp().unwrap().timeline_groups[0].label, effectcraft_engine::color::Label::Fuchsia);
    h.state_mut().session.execute("layer.select", json!({"layers":[other]})).unwrap();
    h.run_steps(3);
    click(&mut h, &format!("timeline.group.{first}.name"), PointerButton::Secondary);
    click(&mut h, &format!("timeline.group.{first}.addSelected"), PointerButton::Primary);
    assert!(h.state().session.active_comp().unwrap().timeline_groups[0].layers.iter().any(|l| l.0 == other));
    click(&mut h, &format!("timeline.group.{first}.name"), PointerButton::Secondary);
    click(&mut h, &format!("timeline.group.{first}.removeSelected"), PointerButton::Primary);
    assert!(!h.state().session.active_comp().unwrap().timeline_groups.iter().any(|g| g.layers.iter().any(|l| l.0 == other)));
    click(&mut h, &format!("timeline.group.{first}.name"), PointerButton::Secondary);
    click(&mut h, &format!("timeline.group.{first}.dissolve"), PointerButton::Primary);
    assert!(h.state().auto.find(&format!("timeline.layer.{upper}.row")).is_some());
    assert!(h.state().auto.find(&format!("timeline.rig.{rig}.name")).is_some());
    assert_eq!(h.state().session.active_comp().unwrap().layers.len(), 4);
}
