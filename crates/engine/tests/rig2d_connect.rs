use std::sync::Arc;

use effectcraft_engine::{
    Session,
    geom::{Mat4, vec3},
    project::{
        ItemId, LayerId, Project, Value as PropertyValue,
        rig2d::{BendDirection, DrivenEnd},
    },
    render::{EvalCtx, RenderOpts},
    time::Tick,
};
use serde_json::{Value, json};

fn command(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute_checked(id, params).unwrap()
}

fn set(s: &mut Session, layer: LayerId, path: &str, value: Value) {
    command(s, "prop.set", json!({"layer":layer.0,"path":path,"value":value}));
}

fn fixture(parented: bool) -> (Session, ItemId, LayerId, LayerId, LayerId) {
    let mut s = Session::default();
    command(&mut s, "comp.new", json!({"width":320,"height":240,"duration":2,"frameRate":24}));
    let cid = s.active_comp_id().unwrap();
    let body = LayerId(command(&mut s, "layer.newNull", json!({"name":"Body"}))["layer"].as_u64().unwrap());
    let mut solids = Vec::new();
    for (name, color) in [("Upper", [1.0, 0.1, 0.2]), ("Lower", [0.2, 0.5, 1.0])] {
        solids.push(LayerId(command(&mut s, "layer.newSolid", json!({"name":name,"width":100,"height":20,"color":color}))["layer"].as_u64().unwrap()));
    }
    let (upper, lower) = (solids[0], solids[1]);
    set(&mut s, body, "transform/anchor", json!([0, 0, 0]));
    set(&mut s, body, "transform/position", json!([30, 15, 0]));
    set(&mut s, body, "transform/rotation", json!(10));
    set(&mut s, body, "transform/scale", json!([110, 110, 100]));
    set(&mut s, upper, "transform/anchor", json!([10, 10, 0]));
    set(&mut s, upper, "transform/position", json!([60, 90, 0]));
    set(&mut s, upper, "transform/rotation", json!(25));
    set(&mut s, upper, "transform/scale", json!([120, 120, 100]));
    set(&mut s, lower, "transform/anchor", json!([5, 10, 0]));
    set(&mut s, lower, "transform/position", json!([150, 130, 0]));
    set(&mut s, lower, "transform/rotation", json!(-15));
    set(&mut s, lower, "transform/scale", json!([90, 90, 100]));
    command(&mut s, "layer.setParent", json!({"layers":[upper.0],"parent":body.0}));
    if parented {
        command(&mut s, "layer.setParent", json!({"layers":[lower.0],"parent":upper.0}));
    }
    s.mark_saved();
    (s, cid, body, upper, lower)
}

fn matrix(project: &Project, cid: ItemId, id: LayerId, time: Tick) -> Mat4 {
    let comp = project.comp(cid).unwrap();
    EvalCtx::new(project, cid, comp, time).world_matrix(comp.layer(id).unwrap())
}

fn assert_pose(before: &Project, after: &Project, cid: ItemId) {
    let comp = before.comp(cid).unwrap();
    assert_eq!(comp.layers.len(), after.comp(cid).unwrap().layers.len());
    for layer in &comp.layers {
        for time in [Tick::ZERO, Tick::from_seconds_f64(1.0)] {
            let a = matrix(before, cid, layer.id, time);
            let b = matrix(after, cid, layer.id, time);
            for (a, b) in a.0.iter().flatten().zip(b.0.iter().flatten()) {
                assert!((a - b).abs() < 1e-9, "pose changed: {a} vs {b}");
            }
        }
    }
}

fn failed(s: &mut Session, params: Value) {
    let before = s.project.clone();
    let state = serde_json::to_value(&s.state).unwrap();
    let revision = s.revision;
    let undo = s.history.undo.len();
    let redo = s.history.redo.len();
    assert!(s.execute_checked("rig.connect2d", params).is_err());
    assert!(Arc::ptr_eq(&before, &s.project));
    assert_eq!(serde_json::to_value(&s.state).unwrap(), state);
    assert_eq!(s.revision, revision);
    assert_eq!(s.history.undo.len(), undo);
    assert_eq!(s.history.redo.len(), redo);
}

#[test]
fn connects_unparented_layers_using_pick_whip_compensation_as_one_edit() {
    let (mut s, cid, body, upper, lower) = fixture(false);
    let before = s.project.clone();
    let undo = s.history.undo.len();
    let image = s.render(cid, Tick::ZERO, RenderOpts::default());
    let result = command(&mut s, "rig.connect2d", json!({"layers":[lower.0,upper.0],"root":upper.0,"end_point":[95,10]}));
    let after = s.project.clone();
    assert_eq!(s.history.undo.len(), undo + 1);
    assert!(s.is_dirty());
    let comp = s.project.comp(cid).unwrap();
    let rig = &comp.rigs2d[0];
    assert_eq!(rig.layers, [upper, lower]);
    assert_eq!(rig.end_point, [95.0, 10.0]);
    assert_eq!(result["rig"], rig.id.0);
    assert_eq!(result["control"], rig.target.uid);
    assert_eq!(comp.layer(upper).unwrap().parent, Some(body));
    assert_eq!(comp.layer(lower).unwrap().parent, Some(upper));
    assert_pose(&before, &after, cid);
    let rendered = s.render(cid, Tick::ZERO, RenderOpts::default());
    assert_eq!(image.data, rendered.data);
    let target = matrix(&before, cid, lower, Tick::ZERO).apply(vec3(95.0, 10.0, 0.0));
    assert_eq!(rig.target.value, PropertyValue::Vec2([target.x, target.y]));
    // Rest geometry is in the body's space, not composition pixels.
    let inv_body = matrix(&before, cid, body, Tick::ZERO).inverse().unwrap();
    let shoulder = matrix(&before, cid, upper, Tick::ZERO).apply(vec3(10.0, 10.0, 0.0));
    let elbow = matrix(&before, cid, lower, Tick::ZERO).apply(vec3(5.0, 10.0, 0.0));
    for (stored, world) in rig.rest_joints.iter().zip([shoulder, elbow, target]) {
        let expected = inv_body.apply(world);
        assert!((stored[0] - expected.x).abs() < 1e-9);
        assert!((stored[1] - expected.y).abs() < 1e-9);
    }
    let loaded = Project::from_json(&s.project.to_file_json().unwrap()).unwrap();
    assert_eq!(*s.project, loaded);
    assert!(s.undo());
    assert_eq!(s.project, before);
    assert!(!s.is_dirty());
    failed(&mut s, json!({"layers":[upper.0,lower.0],"root":upper.0}));
    assert!(s.redo());
    assert_eq!(s.project, after);
    assert!(s.is_dirty());
}

#[test]
fn anisotropic_connect_preserves_pose_and_target_edits_undo_without_touching_properties() {
    for parented in [false, true] {
        let (mut s, cid, _, upper, lower) = fixture(parented);
        set(&mut s, upper, "transform/scale", if parented { json!([150, 75, 100]) } else { json!([100, 100, 100]) });
        set(&mut s, lower, "transform/scale", json!([63.3, 61.5, 100]));
        if parented {
            let tr = Arc::make_mut(&mut s.project).comp_mut(cid).unwrap().layer_mut(upper).unwrap().transform_mut().unwrap();
            let rotation = tr.get_mut("rotation").unwrap();
            rotation.set_animated(true, Tick::ZERO);
            rotation.set_value_at(Tick::from_seconds_f64(1.0), PropertyValue::Scalar(45.0));
        }
        let before = s.project.clone();
        let pixels = s.render(cid, Tick::ZERO, RenderOpts::default());
        let undo = s.history.undo.len();
        let reply = command(&mut s, "rig.connect2d", json!({"layers":[lower.0,upper.0],"root":upper.0,"end_point":[95,10]}));
        let connected = s.project.clone();
        assert_pose(&before, &connected, cid);
        assert_eq!(pixels.data, s.render(cid, Tick::ZERO, RenderOpts::default()).data);
        assert_eq!(s.history.undo.len(), undo + 1);
        let c = s.project.comp(cid).unwrap();
        let r = &c.rigs2d[0];
        let rest = r.rest_joints;
        let l1 = (rest[1][0] - rest[0][0]).hypot(rest[1][1] - rest[0][1]);
        let l2 = (rest[2][0] - rest[1][0]).hypot(rest[2][1] - rest[1][1]);
        let parent = c.layer(upper).unwrap().parent.unwrap();
        let goal = matrix(&s.project, cid, parent, Tick::ZERO).apply(vec3(rest[0][0] + 0.8 * l1.max(l2), rest[0][1] + 0.6 * l1.max(l2), 0.0));
        command(&mut s, "rig.setTarget2d", json!({"rig":reply["rig"],"position":[goal.x,goal.y]}));
        let moved = s.project.clone();
        let tip = matrix(&s.project, cid, lower, Tick::ZERO).apply(vec3(95.0, 10.0, 0.0));
        assert!((tip.x - goal.x).hypot(tip.y - goal.y) < 1e-8);
        assert_eq!(moved.comp(cid).unwrap().layers, connected.comp(cid).unwrap().layers);
        let reopened = Project::from_json(&moved.to_file_json().unwrap()).unwrap();
        assert_eq!(matrix(&moved, cid, lower, Tick::ZERO), matrix(&reopened, cid, lower, Tick::ZERO));
        assert!(s.undo());
        assert_eq!(s.project, connected);
        assert!(s.undo());
        assert_eq!(s.project, before);
        assert!(s.redo());
        assert!(s.redo());
        assert_eq!(s.project, moved);
    }
}

#[test]
fn existing_parent_chain_resolves_both_selection_orders_and_bend_directions() {
    for reversed in [false, true] {
        for (sign, expected) in [(1, BendDirection::Positive), (-1, BendDirection::Negative)] {
            for driven in ["root", "tip"] {
                let (mut s, cid, _, upper, lower) = fixture(true);
                let ids = if reversed { [lower.0, upper.0] } else { [upper.0, lower.0] };
                command(&mut s, "layer.select", json!({"layers":ids}));
                let before = s.project.clone();
                command(&mut s, "rig.connect2d", json!({"end_point":[95,10],"bend_direction":sign,"driven_end":driven}));
                let comp = s.project.comp(cid).unwrap();
                let rig = &comp.rigs2d[0];
                assert_eq!(rig.layers, [upper, lower]);
                assert_eq!(rig.bend_direction, expected);
                assert_eq!(comp.layers, before.comp(cid).unwrap().layers);
                let (id, point, expected_end) =
                    if driven == "root" { (upper, vec3(10.0, 10.0, 0.0), DrivenEnd::Root) } else { (lower, vec3(95.0, 10.0, 0.0), DrivenEnd::Tip) };
                let target = matrix(&before, cid, id, Tick::ZERO).apply(point);
                assert_eq!(rig.driven_end, expected_end);
                assert_eq!(rig.target.value, PropertyValue::Vec2([target.x, target.y]));
            }
        }
    }
}

#[test]
fn invalid_arguments_and_overlapping_rigs_leave_project_and_history_unchanged() {
    let (mut s, _, body, upper, lower) = fixture(false);
    let good = json!({"layers":[upper.0,lower.0],"root":upper.0,"end_point":[95,10]});
    for params in [
        json!({"layers":[],"end_point":[95,10]}),
        json!({"layers":[upper.0,lower.0],"end_point":[95,10]}),
        json!({"layers":[upper.0],"end_point":[95,10]}),
        json!({"layers":[upper.0,upper.0],"root":upper.0,"end_point":[95,10]}),
        json!({"layers":[upper.0,lower.0,body.0],"root":upper.0,"end_point":[95,10]}),
        json!({"layers":[upper.0,"missing"],"root":upper.0,"end_point":[95,10]}),
    ] {
        failed(&mut s, params);
    }
    for (key, value) in [
        ("root", json!(body.0)),
        ("end_point", json!([1])),
        ("end_point", json!([1, 2, 3])),
        ("end_point", json!([null, 2])),
        ("end_point", json!(["NaN", 2])),
        ("end_point", json!([5, 10])),
        ("end_point", json!(null)),
        ("bend_direction", json!(0)),
        ("bend_direction", json!("negative")),
        ("driven_end", json!("middle")),
        ("layers", json!(42)),
    ] {
        let mut params = good.clone();
        params[key] = value;
        failed(&mut s, params);
    }
    command(&mut s, "rig.connect2d", good.clone());
    failed(&mut s, good);
}

#[test]
fn unsupported_parenting_and_bad_geometry_are_atomic_errors() {
    for case in ["locked", "3d", "singular", "animated", "shear", "cycle", "exhausted", "nonfinite", "missing_ancestor"] {
        let (mut s, cid, body, upper, lower) = fixture(false);
        match case {
            "locked" => {
                command(&mut s, "layer.setSwitch", json!({"layers":[lower.0],"switch":"locked","value":true}));
            }
            "3d" => {
                command(&mut s, "layer.setSwitch", json!({"layers":[body.0],"switch":"threeD","value":true}));
            }
            "singular" => set(&mut s, upper, "transform/scale", json!([0, 100, 100])),
            "animated" => {
                Arc::make_mut(&mut s.project)
                    .comp_mut(cid)
                    .unwrap()
                    .layer_mut(lower)
                    .unwrap()
                    .props
                    .prop_mut("transform/position")
                    .unwrap()
                    .set_animated(true, Tick::ZERO);
            }
            "shear" => set(&mut s, upper, "transform/scale", json!([150, 60, 100])),
            "cycle" => {
                Arc::make_mut(&mut s.project).comp_mut(cid).unwrap().layer_mut(body).unwrap().parent = Some(upper);
            }
            "exhausted" => {
                Arc::make_mut(&mut s.project).next_id = u64::MAX;
            }
            "nonfinite" => {
                Arc::make_mut(&mut s.project).comp_mut(cid).unwrap().layer_mut(lower).unwrap().props.prop_mut("transform/scale").unwrap().value =
                    PropertyValue::Vec3([f64::NAN, 100.0, 100.0]);
            }
            "missing_ancestor" => {
                Arc::make_mut(&mut s.project).comp_mut(cid).unwrap().layer_mut(body).unwrap().parent = Some(LayerId(999_999));
            }
            _ => unreachable!(),
        }
        failed(&mut s, json!({"layers":[upper.0,lower.0],"root":upper.0,"end_point":[95,10]}));
    }
}

#[test]
fn conflicting_root_is_rejected_and_existing_animation_is_not_rewritten() {
    let (mut s, cid, _, upper, lower) = fixture(true);
    failed(&mut s, json!({"layers":[upper.0,lower.0],"root":lower.0,"end_point":[95,10]}));
    let p = Arc::make_mut(&mut s.project).comp_mut(cid).unwrap().layer_mut(upper).unwrap().props.prop_mut("transform/rotation").unwrap();
    p.set_animated(true, Tick::ZERO);
    p.set_value_at(Tick::from_seconds_f64(1.0), PropertyValue::Scalar(70.0));
    s.set_time(Tick::from_seconds_f64(1.0));
    // Explicit comp requests must use that composition's playhead, even when inactive.
    command(&mut s, "comp.new", json!({"name":"Other","width":64,"height":48,"duration":2,"frameRate":24}));
    let before = s.project.clone();
    command(&mut s, "rig.connect2d", json!({"comp":cid.0,"layers":[lower.0,upper.0],"end_point":[95,10]}));
    assert_eq!(s.project.comp(cid).unwrap().layers, before.comp(cid).unwrap().layers);
    assert_pose(&before, &s.project, cid);
    let tip = matrix(&before, cid, lower, Tick::from_seconds_f64(1.0)).apply(vec3(95.0, 10.0, 0.0));
    assert_eq!(s.project.comp(cid).unwrap().rigs2d[0].target.value, PropertyValue::Vec2([tip.x, tip.y]));
}
