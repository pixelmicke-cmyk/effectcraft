//! Persistence-stage rig edits use the ordinary document history, without rendering IK.
use effectcraft_engine::{
    Session,
    project::{Value, rig2d::Rig2d},
};
use serde_json::json;

fn fixture() -> (Session, effectcraft_engine::project::ItemId) {
    let mut session = Session::default();
    session.execute("comp.new", json!({"width":64,"height":48,"duration":2,"frameRate":24})).unwrap();
    for name in ["Upper", "Lower"] {
        session.execute("layer.newNull", json!({"name":name})).unwrap();
    }
    let cid = session.active_comp_id().unwrap();
    let comp = session.project.comp(cid).unwrap();
    let layers = [comp.layers[0].id, comp.layers[1].id];
    session
        .edit("Prepare rig data", None, |project, _| {
            let rig = Rig2d::new(project, layers, [[0.0, 0.0], [10.0, 0.0], [20.0, 0.0]], [10.0, 0.0], [20.0, 0.0])?;
            project.comp_mut(cid).unwrap().rigs2d.push(rig);
            Ok(())
        })
        .unwrap();
    (session, cid)
}

#[test]
fn rig_control_edits_support_undo_redo_and_dirty_tracking() {
    let (mut session, cid) = fixture();
    session.mark_saved();
    let before = session.project.clone();
    session
        .edit("Edit rig target", None, |project, _| {
            project.comp_mut(cid).unwrap().rigs2d[0].target.value = Value::Vec2([5.0, 8.0]);
            Ok(())
        })
        .unwrap();
    assert!(session.is_dirty());
    assert!(!session.project.same_content(&before));
    assert!(!session.project.comp(cid).unwrap().same_pixels(before.comp(cid).unwrap()));
    let after = session.project.clone();
    assert!(session.undo());
    assert!(!session.is_dirty());
    assert_eq!(session.project, before);
    assert!(session.redo());
    assert!(session.is_dirty());
    assert_eq!(session.project, after);
    assert_eq!(session.project.comp(cid).unwrap().layers.len(), 2);
}

#[test]
fn composition_duplicate_allocates_new_rig_ids_and_remaps_bones() {
    let (mut session, cid) = fixture();
    let original = session.project.comp(cid).unwrap().rigs2d[0].clone();
    let reply = session.execute("project.duplicate", json!({"items":[cid.0]})).unwrap();
    let new_cid = effectcraft_engine::project::ItemId(reply["items"][0].as_u64().unwrap());
    let comp = session.project.comp(new_cid).unwrap();
    let rig = &comp.rigs2d[0];
    assert_ne!(rig.id, original.id);
    assert_ne!(rig.target.uid, original.target.uid);
    assert!(rig.layers.iter().all(|id| comp.layer(*id).is_some()));
    assert!(rig.layers.iter().all(|id| !original.layers.contains(id)));
    assert_eq!(rig.layers, [comp.layers[0].id, comp.layers[1].id]);
    assert_eq!(rig.rest_joints, original.rest_joints);
    assert_eq!(rig.target.value, original.target.value);
    assert_eq!(comp.layers.len(), 2);
    session.project.to_file_json().unwrap();
    assert!(session.undo());
    assert!(session.project.comp(new_cid).is_none());
    assert!(session.redo());
    assert!(session.project.comp(new_cid).is_some());
}

#[test]
fn checked_image_reads_report_unsupported_rig_data_without_modifying_project() {
    let (session, cid) = fixture();
    let before = session.project.clone();
    // The persistence-only fixture intentionally has no bone parenting yet.
    for transparent in [false, true] {
        let error = session.render_rgba8_alpha(cid, session.time(), 64, transparent).unwrap_err().to_string();
        assert!(error.contains("2D IK"), "{error}");
    }
    assert_eq!(session.project, before);
}
