use effectcraft_engine::{Session, geom::vec3, project::Value, render::EvalCtx, time::Tick};
use serde_json::json;
use std::sync::Arc;

fn fixture() -> (Session, u64) {
    let mut s = Session::default();
    s.execute_checked("comp.new", json!({"width":256,"height":128,"duration":2,"frameRate":24})).unwrap();
    for (name, width, position) in [("Upper", 50, [40, 64, 0]), ("Lower", 40, [90, 64, 0])] {
        s.execute_checked("layer.newSolid", json!({"name":name,"width":width,"height":8})).unwrap();
        s.execute_checked("prop.set", json!({"layer":name,"path":"transform/position","value":position})).unwrap();
    }
    let result = s.execute_checked("rig.connect2d", json!({"layers":["Upper","Lower"],"root":"Upper","end_point":[40,4]})).unwrap();
    s.mark_saved();
    (s, result["rig"].as_u64().unwrap())
}

#[test]
fn target_updates_are_merged_undoable_and_never_write_layer_rotations() {
    let (mut s, rig) = fixture();
    let cid = s.active_comp_id().unwrap();
    let before = s.project.clone();
    let undo = s.history.undo.len();
    for position in [[100.0, 55.0], [90.0, 40.0]] {
        s.execute_checked("rig.setTarget2d", json!({"rig":rig,"position":position,"merge":"first-drag"})).unwrap();
        let c = s.project.comp(cid).unwrap();
        let ctx = EvalCtx::new(&s.project, cid, c, s.time());
        let lower = c.layer(c.rigs2d[0].layers[1]).unwrap();
        let tip = ctx.world_matrix(lower).apply(vec3(40.0, 4.0, 0.0));
        assert!((tip.x - position[0]).hypot(tip.y - position[1]) < 1e-8);
        assert_eq!(c.layers, before.comp(cid).unwrap().layers);
    }
    assert_eq!(s.history.undo.len(), undo + 1);
    let first = s.project.clone();
    s.execute_checked("rig.setTarget2d", json!({"rig":rig,"position":[95,45],"merge":"second-drag"})).unwrap();
    assert_eq!(s.history.undo.len(), undo + 2);
    let second = s.project.clone();
    assert!(s.undo());
    assert_eq!(s.project, first);
    assert!(s.undo());
    assert_eq!(s.project, before);
    assert!(!s.is_dirty());
    assert!(s.redo());
    assert!(s.redo());
    assert_eq!(s.project, second);
    let loaded = effectcraft_engine::project::Project::from_json(&s.project.to_file_json().unwrap()).unwrap();
    assert_eq!(loaded, *s.project);
}

#[test]
fn selection_is_independent_and_invalid_edits_are_atomic() {
    let (mut s, rig) = fixture();
    let cid = s.active_comp_id().unwrap();
    let layers = s.state.selected_layers.clone();
    let before = s.project.clone();
    let undo = s.history.undo.len();
    let revision = s.revision;
    s.execute_checked("rig.select2d", json!({"rig":rig})).unwrap();
    assert_eq!(s.state.selected_rig2d.unwrap().0, rig);
    assert_eq!(s.state.selected_layers, layers);
    assert!(Arc::ptr_eq(&s.project, &before));
    assert_eq!(s.history.undo.len(), undo);
    assert!(!s.is_dirty());
    for params in [
        json!({"rig":999999,"position":[95,45]}),
        json!({"rig":rig,"position":[null,0]}),
        json!({"rig":rig,"position":[1]}),
        json!({"rig":rig,"position":[1,2,3]}),
    ] {
        assert!(s.execute_checked("rig.setTarget2d", params).is_err());
        assert!(Arc::ptr_eq(&s.project, &before));
        assert_eq!(s.history.undo.len(), undo);
        assert_eq!(s.revision, revision);
    }
    s.execute_checked("rig.select2d", json!({"rig":null})).unwrap();
    assert!(s.state.selected_rig2d.is_none());
    s.execute_checked("rig.select2d", json!({"rig":rig})).unwrap();
    assert!(s.undo());
    assert!(s.state.selected_rig2d.is_none());
    assert!(s.redo());
    let ids = s.project.comp(cid).unwrap().rigs2d[0].layers;
    s.execute_checked("layer.setSwitch", json!({"layers":[ids[0].0],"switch":"locked","value":true})).unwrap();
    let locked = s.project.clone();
    assert!(s.execute_checked("rig.setTarget2d", json!({"rig":rig,"position":[90,40]})).is_err());
    assert!(Arc::ptr_eq(&s.project, &locked));
}

#[test]
fn animated_target_uses_composition_time_and_old_editor_state_still_loads() {
    let (mut s, rig) = fixture();
    let cid = s.active_comp_id().unwrap();
    let target = &mut Arc::make_mut(&mut s.project).comp_mut(cid).unwrap().rigs2d[0].target;
    target.set_animated(true, Tick::ZERO);
    s.set_time(Tick::from_seconds_f64(0.5));
    let before = s.project.clone();
    s.execute_checked("rig.setTarget2d", json!({"rig":rig,"position":[90,40]})).unwrap();
    let target = &s.project.comp(cid).unwrap().rigs2d[0].target;
    assert_eq!(target.keys.len(), 2);
    assert_eq!(target.keys[1].time, Tick::from_seconds_f64(0.5));
    assert_eq!(target.keys[1].value, Value::Vec2([90.0, 40.0]));
    assert_eq!(target.keys[0], before.comp(cid).unwrap().rigs2d[0].target.keys[0]);
    let mut state = serde_json::to_value(&s.state).unwrap();
    state.as_object_mut().unwrap().remove("selected_rig2d");
    let restored: effectcraft_engine::EditorState = serde_json::from_value(state).unwrap();
    assert!(restored.selected_rig2d.is_none());
}
