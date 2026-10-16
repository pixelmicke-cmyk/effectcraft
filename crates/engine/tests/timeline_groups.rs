use effectcraft_engine::{
    Session,
    geom::vec3,
    project::{LayerId, Project},
    render::{EvalCtx, RenderOpts},
    time::Tick,
};
use serde_json::{Value, json};

fn fixture() -> (Session, u64) {
    let mut s = Session::default();
    s.execute("comp.new", json!({"width":256,"height":128,"duration":3,"frameRate":24})).unwrap();
    for (name, pos) in [("Upper", [40, 64, 0]), ("Lower", [90, 64, 0]), ("Other", [10, 10, 0]), ("Fourth", [20, 10, 0])] {
        s.execute("layer.newSolid", json!({"name":name,"width":40,"height":8})).unwrap();
        s.execute("prop.set", json!({"layer":name,"path":"transform/position","value":pos})).unwrap();
    }
    let rig = s.execute("rig.connect2d", json!({"layers":["Upper","Lower"],"root":"Upper","end_point":[40,4]})).unwrap()["rig"].as_u64().unwrap();
    (s, rig)
}

fn edit(s: &mut Session, command: &str, args: Value) -> Value {
    let before = s.project.clone();
    let cid = s.active_comp_id().unwrap();
    let undo = s.history.undo.len();
    let pixels = s.render(cid, s.time(), RenderOpts::default()).data;
    let result = s.execute(command, args).unwrap();
    let after = s.project.clone();
    assert_eq!(s.history.undo.len(), undo + 1, "{command}");
    assert_eq!(s.active_comp().unwrap().layers, before.comp(cid).unwrap().layers);
    assert_eq!(s.active_comp().unwrap().rigs2d, before.comp(cid).unwrap().rigs2d);
    assert!(s.active_comp().unwrap().same_pixels(before.comp(cid).unwrap()));
    assert_eq!(pixels, s.render(cid, s.time(), RenderOpts::default()).data);
    assert!(s.undo());
    assert_eq!(s.project, before);
    assert!(s.redo());
    assert_eq!(s.project, after);
    result
}

#[test]
fn groups_edit_membership_labels_names_collapse_and_dissolve_without_changing_layers() {
    let (mut s, _) = fixture();
    let first = edit(&mut s, "timeline.group.create", json!({"layers":["Upper","Lower"],"name":"Arm"}))["group"].as_u64().unwrap();
    let second = edit(&mut s, "timeline.group.create", json!({"layers":["Other","Fourth"],"name":"Body"}))["group"].as_u64().unwrap();
    assert_ne!(first, second);
    edit(&mut s, "timeline.group.collapse", json!({"group":first,"collapsed":true}));
    assert!(!s.active_comp().unwrap().timeline_groups[1].collapsed);
    edit(&mut s, "timeline.group.collapse", json!({"group":second,"collapsed":true}));
    edit(&mut s, "timeline.group.rename", json!({"group":first,"name":"Left Arm"}));
    edit(&mut s, "timeline.group.label", json!({"group":first,"label":"Fuchsia"}));
    edit(&mut s, "timeline.group.add", json!({"group":first,"layers":["Other"]}));
    assert_eq!(s.active_comp().unwrap().timeline_groups[1].layers.len(), 1);
    edit(&mut s, "timeline.group.remove", json!({"group":first,"layers":["Lower"]}));
    let loaded = Project::from_json(&s.project.to_file_json().unwrap()).unwrap();
    assert_eq!(loaded, *s.project);
    let mut old = (*s.project).clone();
    old.comp_mut(s.active_comp_id().unwrap()).unwrap().timeline_groups.clear();
    let legacy = old.to_file_json().unwrap();
    assert!(!legacy.contains("timeline_groups"));
    assert!(Project::from_json(&legacy).unwrap().comp(s.active_comp_id().unwrap()).unwrap().timeline_groups.is_empty());
    edit(&mut s, "timeline.group.dissolve", json!({"group":first}));
    assert_eq!(s.active_comp().unwrap().timeline_groups.len(), 1);
}

#[test]
fn collapsed_group_keeps_animated_ik_and_parenting_active() {
    let (mut s, rig) = fixture();
    let uid = s.active_comp().unwrap().rigs2d[0].target.uid;
    for (time, value) in [(0, json!([100, 60])), (1, json!([85, 40]))] {
        s.execute("prop.addKey", json!({"rig":rig,"prop":uid,"time":time,"value":value})).unwrap();
    }
    let group = edit(&mut s, "timeline.group.create", json!({"layers":["Upper","Lower"]}))["group"].as_u64().unwrap();
    edit(&mut s, "timeline.group.collapse", json!({"group":group,"collapsed":true}));
    for time in [0.0, 0.5, 1.0] {
        s.set_time(Tick::from_seconds_f64(time));
        let comp = s.active_comp().unwrap();
        let ctx = EvalCtx::new(&s.project, s.active_comp_id().unwrap(), comp, s.time());
        let rig = &comp.rigs2d[0];
        let target = rig.target.value_at(s.time()).as_vec2();
        let lower = comp.layer(rig.layers[1]).unwrap();
        assert_eq!(lower.parent, Some(rig.layers[0]));
        let tip = ctx.world_matrix(lower).apply(vec3(40.0, 4.0, 0.0));
        assert!((tip.x - target[0]).hypot(tip.y - target[1]) < 1e-6);
    }
}

#[test]
fn ids_remap_on_duplicate_import_and_deleted_members_undo_together() {
    let (mut s, _) = fixture();
    let group = s.execute("timeline.group.create", json!({"layers":["Other"]})).unwrap()["group"].as_u64().unwrap();
    let cid = s.active_comp_id().unwrap();
    let original = s.active_comp().unwrap().timeline_groups[0].clone();
    let copy = s.execute("project.duplicate", json!({"items":[cid.0]})).unwrap()["items"][0].as_u64().unwrap();
    let copied = &s.project.comp(effectcraft_engine::project::ItemId(copy)).unwrap().timeline_groups[0];
    assert_ne!(copied.id, original.id);
    assert_ne!(copied.layers, original.layers);
    let mut imported = (*s.project).clone();
    effectcraft_engine::project::essential::offset_ids(&mut imported, 100_000);
    let loaded = Project::from_json(&imported.to_file_json().unwrap()).unwrap();
    assert_eq!(loaded, imported);
    s.execute("layer.select", json!({"layers":["Other"]})).unwrap();
    let before = s.project.clone();
    s.execute("edit.clear", json!({})).unwrap();
    assert!(s.project.comp(cid).unwrap().timeline_groups.iter().find(|g| g.id.0 == group).unwrap().layers.is_empty());
    assert!(s.undo());
    assert_eq!(s.project, before);
    let mut repair = (*s.project).clone();
    repair.comp_mut(cid).unwrap().timeline_groups[0].layers.push(LayerId(u64::MAX));
    let (_, notes) = Project::from_json_repaired(&repair.to_json()).unwrap();
    assert!(notes.iter().any(|n| n.contains("timeline group")));
}

#[test]
fn invalid_group_edits_are_atomic_and_corrupt_ids_are_rejected() {
    let (mut s, _) = fixture();
    let group = s.execute("timeline.group.create", json!({"layers":["Other"]})).unwrap()["group"].as_u64().unwrap();
    let before = s.project.clone();
    let undo = s.history.undo.len();
    for (cmd, args) in [
        ("timeline.group.rename", json!({"group":group,"name":"  "})),
        ("timeline.group.label", json!({"group":group,"label":"invalid"})),
        ("timeline.group.add", json!({"group":group,"layers":["missing"]})),
        ("timeline.group.dissolve", json!({"group":u64::MAX})),
        ("timeline.group.remove", json!({"group":"invalid","layers":["Other"]})),
        ("timeline.group.add", json!({"group":group,"layers":"Other"})),
        ("timeline.group.rename", json!({"group":group})),
    ] {
        assert!(s.execute(cmd, args).is_err());
        assert_eq!(s.project, before);
        assert_eq!(s.history.undo.len(), undo);
    }
    let mut corrupt = (*s.project).clone();
    let cid = s.active_comp_id().unwrap();
    corrupt.comp_mut(cid).unwrap().timeline_groups[0].id.0 = s.active_comp().unwrap().layers[0].id.0;
    assert!(Project::from_json(&corrupt.to_json()).is_err());
}
