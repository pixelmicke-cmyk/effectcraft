use effectcraft_engine::{
    Session,
    keyframe::{Keyframe, Value},
    time::Tick,
};
use serde_json::json;

fn fixture() -> Session {
    let mut s = Session::default();
    s.execute("comp.new", json!({"width":256,"height":128,"duration":3,"frameRate":24})).unwrap();
    s.execute("layer.newSolid", json!({"name":"Upper","width":50,"height":8})).unwrap();
    s.execute("layer.newSolid", json!({"name":"Lower","width":40,"height":8})).unwrap();
    s
}

#[test]
fn replaces_only_value_preserving_subframe_time_and_all_destination_metadata() {
    let mut s = fixture();
    let cid = s.active_comp_id().unwrap();
    let owner = s.active_comp().unwrap().layers[0].id;
    let time = Tick(123456789);
    let mut destination = Keyframe::new(time, Value::Scalar(77.0)).eased();
    destination.spatial_in = [1.0, 2.0, 3.0];
    destination.spatial_out = [4.0, 5.0, 6.0];
    destination.spatial_auto = false;
    destination.continuous = true;
    destination.label = 5;
    let uid = {
        let pr = std::sync::Arc::make_mut(&mut s.project).comp_mut(cid).unwrap().layer_mut(owner).unwrap().props.prop_mut("transform/opacity").unwrap();
        pr.keys = vec![Keyframe::new(Tick::ZERO, Value::Scalar(23.0)), destination.clone()];
        pr.uid
    };
    s.mark_saved();
    s.set_time(Tick::from_seconds_f64(1.5));
    let undo = s.history.undo.len();
    let before = s.project.clone();
    s.execute("keys.copyValue", json!({"layer":owner.0,"prop":uid,"time":0})).unwrap();
    assert_eq!(s.project, before);
    assert_eq!(s.history.undo.len(), undo);
    s.execute("keys.pasteValue", json!({"layer":owner.0,"prop":uid,"time":time.seconds()})).unwrap();
    destination.value = Value::Scalar(23.0);
    assert_eq!(s.active_comp().unwrap().animation_property(owner, uid).unwrap().keys[1], destination);
    let after = s.project.clone();
    assert!(s.undo());
    assert_eq!(s.project, before);
    assert!(s.redo());
    assert_eq!(s.project, after);
    let unchanged = s.project.clone();
    assert!(s.execute("keys.pasteValue", json!({"layer":owner.0,"path":"transform/position","time":1})).is_err());
    assert!(s.execute("keys.copyValue", json!({"layer":owner.0,"prop":uid,"time":2})).is_err());
    assert_eq!(s.project, unchanged);
}

#[test]
fn ik_vec2_values_create_loop_key_at_cti_with_single_undo_and_roundtrip() {
    let mut s = fixture();
    s.execute("prop.set", json!({"layer":"Lower","path":"transform/position","value":[50,0,0]})).unwrap();
    let rig = s.execute("rig.connect2d", json!({"layers":["Upper","Lower"],"root":"Upper","end_point":[40,4]})).unwrap()["rig"].as_u64().unwrap();
    let uid = s.active_comp().unwrap().rigs2d[0].target.uid;
    for (time, value) in [(0, json!([70, 10])), (1, json!([60, 20])), (2, json!([50, 30]))] {
        s.execute("prop.addKey", json!({"rig":rig,"prop":uid,"time":time,"value":value})).unwrap();
    }
    s.execute("keys.select", json!({"keys":[{"rig":rig,"prop":uid,"time":0}]})).unwrap();
    s.execute("keys.copyValue", json!({})).unwrap();
    s.execute("keys.pasteValue", json!({"rig":rig,"prop":uid,"time":2})).unwrap();
    assert_eq!(s.active_comp().unwrap().rigs2d[0].target.keys[2].value, Value::Vec2([70.0, 10.0]));
    s.set_time(Tick::from_seconds_f64(2.5));
    let before = s.project.clone();
    s.execute("keys.pasteValue", json!({"rig":rig,"prop":uid})).unwrap();
    let target = &s.active_comp().unwrap().rigs2d[0].target;
    assert_eq!(target.keys.len(), 4);
    assert_eq!(target.keys[3].time, s.time());
    assert_eq!(target.value_at(s.time()), Value::Vec2([70.0, 10.0]));
    let after = s.project.clone();
    assert!(s.undo());
    assert_eq!(s.project, before);
    assert!(s.redo());
    assert_eq!(s.project, after);
    let reopened = effectcraft_engine::project::Project::from_json(&s.project.to_file_json().unwrap()).unwrap();
    assert_eq!(reopened, *s.project);
}

#[test]
fn new_layer_key_uses_layer_time_and_failed_paste_is_atomic() {
    let mut s = fixture();
    s.execute("prop.addKey", json!({"layer":"Upper","path":"transform/opacity","time":0,"value":35})).unwrap();
    s.execute("keys.copyValue", json!({"layer":"Upper","path":"transform/opacity","time":0})).unwrap();
    let cid = s.active_comp_id().unwrap();
    let owner = s.active_comp().unwrap().layers.iter().find(|l| l.name == "Upper").unwrap().id;
    {
        let layer = std::sync::Arc::make_mut(&mut s.project).comp_mut(cid).unwrap().layer_mut(owner).unwrap();
        layer.start_time = Tick::from_seconds_f64(1.0);
        layer.stretch = 200.0;
    }
    s.set_time(Tick::from_seconds_f64(2.0));
    let expected = s.active_comp().unwrap().animation_time(owner, s.time());
    s.execute("keys.pasteValue", json!({"layer":owner.0,"path":"transform/opacity"})).unwrap();
    let pr = s.active_comp().unwrap().layer(owner).unwrap().props.prop("transform/opacity").unwrap();
    assert_eq!(pr.keys[1].time, expected);
    assert_eq!(pr.keys[1].value, Value::Scalar(35.0));
    std::sync::Arc::make_mut(&mut s.project).comp_mut(cid).unwrap().layer_mut(owner).unwrap().switches.locked = true;
    let before = s.project.clone();
    let undo = s.history.undo.len();
    assert!(s.execute("keys.pasteValue", json!({"layer":owner.0,"path":"transform/opacity","time":2})).is_err());
    assert_eq!(s.project, before);
    assert_eq!(s.history.undo.len(), undo);
}
