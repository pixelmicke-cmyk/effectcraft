use effectcraft_engine::{Session, geom::vec3, render::EvalCtx, time::Tick};
use serde_json::json;

fn fixture() -> (Session, u64, u64) {
    let mut s = Session::default();
    s.execute("comp.new", json!({"width":256,"height":128,"duration":3,"frameRate":24})).unwrap();
    for (name, width, position) in [("Upper", 50, [40, 64, 0]), ("Lower", 40, [90, 64, 0])] {
        s.execute("layer.newSolid", json!({"name":name,"width":width,"height":8})).unwrap();
        s.execute("prop.set", json!({"layer":name,"path":"transform/position","value":position})).unwrap();
    }
    let rig = s.execute("rig.connect2d", json!({"layers":["Upper","Lower"],"root":"Upper","end_point":[40,4]})).unwrap()["rig"].as_u64().unwrap();
    let uid = s.active_comp().unwrap().rigs2d[0].target.uid;
    (s, rig, uid)
}

#[test]
fn rig_label_edit_is_independent_atomic_persistent_and_one_undo() {
    use effectcraft_engine::color::Label;
    let (mut s, rig, _) = fixture();
    s.mark_saved();
    let before = s.project.clone();
    let old_rig = s.active_comp().unwrap().rigs2d[0].clone();
    let bones = s.active_comp().unwrap().layers.clone();
    let undo = s.history.undo.len();
    s.execute("rig.setLabel2d", json!({"rig":rig,"label":"Green"})).unwrap();
    assert!(s.is_dirty());
    assert_eq!(s.history.undo.len(), undo + 1);
    assert_eq!(s.active_comp().unwrap().layers, bones);
    let mut expected = old_rig;
    expected.label = Label::Green;
    assert_eq!(s.active_comp().unwrap().rigs2d[0], expected);
    let after = s.project.clone();
    assert_ne!(before, after);
    let loaded = effectcraft_engine::project::Project::from_json(&s.project.to_file_json().unwrap()).unwrap();
    assert_eq!(loaded, *s.project);
    assert!(s.undo());
    assert_eq!(s.project, before);
    assert!(!s.is_dirty());
    assert!(s.redo());
    assert_eq!(s.project, after);
    assert!(s.execute("rig.setLabel2d", json!({"rig":rig,"label":"not a palette color"})).is_err());
    assert!(s.execute("rig.setLabel2d", json!({"rig":u64::MAX,"label":"Blue"})).is_err());
    assert_eq!(s.project, after);
    assert_eq!(s.history.undo.len(), undo + 1);
}

#[test]
fn existing_property_and_key_commands_animate_rig_in_composition_time() {
    let (mut s, rig, uid) = fixture();
    let cid = s.active_comp_id().unwrap();
    for bone in &mut std::sync::Arc::make_mut(&mut s.project).comp_mut(cid).unwrap().layers {
        bone.start_time = Tick::from_seconds_f64(5.0);
        bone.stretch = 200.0;
    }
    s.execute("rig.select2d", json!({"rig":rig,"timeline":true})).unwrap();
    assert!(s.state.selected_layers.is_empty());
    s.execute("prop.toggleAnimation", json!({"rig":rig,"prop":uid})).unwrap();
    s.set_time(Tick::from_seconds_f64(1.0));
    s.execute("rig.setTarget2d", json!({"rig":rig,"position":[90,40]})).unwrap();
    assert_eq!(s.active_comp().unwrap().rigs2d[0].target.keys.len(), 2);
    let p = s.execute("prop.get", json!({"rig":rig,"prop":uid,"time":0.5})).unwrap();
    assert_eq!(p["value"], json!([100.0, 52.0]));
    s.set_time(Tick::from_seconds_f64(0.5));
    let c = s.active_comp().unwrap();
    let ctx = EvalCtx::new(&s.project, s.active_comp_id().unwrap(), c, s.time());
    let tip = ctx.world_matrix(c.layer(c.rigs2d[0].layers[1]).unwrap()).apply(vec3(40.0, 4.0, 0.0));
    assert!((tip.x - 100.0).hypot(tip.y - 52.0) < 1e-6);
    // The control's composition time is independent of the bones' timing.
    let reply = s.execute("keys.select", json!({"keys":[{"rig":rig,"prop":uid,"time":1.0}],"selectProperties":true,"timeBase":"comp"})).unwrap();
    assert_eq!(reply["keys"][0]["rig"], rig);
    assert_eq!(reply["keys"][0]["compTime"], 1.0);
    assert!(s.state.selected_layers.is_empty());
    assert_eq!(s.state.selected_keys[0].layer.0, rig);
    let before = s.project.clone();
    assert!(s.execute("prop.addKey", json!({"rig":rig,"prop":uid,"time":1e300})).is_err());
    assert!(s.execute("keys.move", json!({"delta":1e300})).is_err());
    assert_eq!(s.project, before);
    s.execute("keys.move", json!({"delta":0.5,"merge":"rig-key-drag"})).unwrap();
    s.execute("keys.move", json!({"delta":0.5,"merge":"rig-key-drag"})).unwrap();
    assert_eq!(s.state.selected_keys[0].time, Tick::from_seconds_f64(2.0));
    assert_eq!(s.active_comp().unwrap().rigs2d[0].target.keys[1].time, Tick::from_seconds_f64(2.0));
    let moved = s.project.clone();
    assert!(s.undo());
    assert_eq!(s.project, before);
    assert!(s.redo());
    assert_eq!(s.project, moved);
    assert_eq!(s.state.selected_keys[0].time, Tick::from_seconds_f64(2.0));
    s.execute("keys.interpolation", json!({"interpolation":"hold"})).unwrap();
    s.execute("keys.easyEase", json!({})).unwrap();
    let before_delete = s.project.clone();
    s.execute("edit.clear", json!({})).unwrap();
    assert_eq!(s.active_comp().unwrap().rigs2d[0].target.keys.len(), 1);
    assert_eq!(s.active_comp().unwrap().layers.len(), 2);
    assert!(s.undo());
    assert_eq!(s.project, before_delete);
    let reopened = effectcraft_engine::project::Project::from_json(&s.project.to_file_json().unwrap()).unwrap();
    assert_eq!(reopened, *s.project);
}

#[test]
fn static_and_animated_target_edits_follow_existing_stopwatch_rules() {
    let (mut s, rig, uid) = fixture();
    let bones = s.active_comp().unwrap().layers.clone();
    s.set_time(Tick::from_seconds_f64(1.0));
    s.execute("prop.set", json!({"rig":rig,"prop":uid,"value":[95,45]})).unwrap();
    assert!(s.active_comp().unwrap().rigs2d[0].target.keys.is_empty());
    s.execute("prop.addKey", json!({"rig":rig,"prop":uid,"time":0,"timeBase":"comp","value":[100,55]})).unwrap();
    s.execute("prop.addKey", json!({"rig":rig,"prop":uid,"time":2,"value":[90,40]})).unwrap();
    s.execute("prop.set", json!({"rig":rig,"prop":uid,"time":1,"timeBase":"comp","value":[95,45]})).unwrap();
    let r = &s.active_comp().unwrap().rigs2d[0];
    assert_eq!(r.target.uid, uid);
    assert_eq!(r.target.keys.len(), 3);
    assert_eq!(r.target.keys[1].time, Tick::from_seconds_f64(1.0));
    assert_eq!(s.active_comp().unwrap().layers, bones);
    let before = s.project.clone();
    assert!(s.execute("prop.set", json!({"rig":rig,"prop":999999,"value":[1,2]})).is_err());
    assert_eq!(s.project, before);
    s.execute("prop.toggleAnimation", json!({"rig":rig,"prop":uid,"value":false})).unwrap();
    assert!(s.active_comp().unwrap().rigs2d[0].target.keys.is_empty());
}

#[test]
fn control_selection_survives_composition_switches_and_legacy_editor_state_loads() {
    let (mut s, rig, uid) = fixture();
    let first = s.active_comp_id().unwrap();
    s.execute("prop.addKey", json!({"rig":rig,"prop":uid,"time":0})).unwrap();
    s.execute("prop.select", json!({"rig":rig,"prop":uid})).unwrap();
    let selection = s.state.selected_keys.clone();
    s.execute("comp.new", json!({"width":32,"height":32,"duration":1})).unwrap();
    s.open_comp(first);
    s.sanitize_state();
    assert_eq!(s.state.selected_rig2d.unwrap().0, rig);
    assert_eq!(s.state.selected_keys, selection);
    let mut state = serde_json::to_value(&s.state).unwrap();
    state.as_object_mut().unwrap().remove("selected_rig2d");
    let _: effectcraft_engine::EditorState = serde_json::from_value(state).unwrap();
    let old = json!({"layers":[],"props":[],"keys":[],"vertices":[]});
    let old: effectcraft_engine::CompSelection = serde_json::from_value(old).unwrap();
    assert!(old.rig2d.is_none());
}
