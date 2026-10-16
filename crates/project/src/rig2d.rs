//! Persistent two-bone rig definitions. No solving or rendering is performed here.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::{LayerId, ParamUi, Project, ProjectError, Property, Value};

/// Shares the project's id namespace with items, layers and property uids.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct RigId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DrivenEnd {
    Root,
    Tip,
}

/// Sign of the perpendicular offset used by the geometry solver (y grows down).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BendDirection {
    Positive,
    Negative,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Rig2d {
    pub id: RigId,
    /// UI label only, independent of the bone layers' labels.
    #[serde(default = "default_label")]
    pub label: effectcraft_color::Label,
    /// Root-to-tip order, independent of layer stacking, selection and driven end.
    pub layers: [LayerId; 2],
    /// Root, middle joint and tip at bind time, in the root layer's parent space
    /// (composition pixels if it has no parent). These define the rest geometry.
    pub rest_joints: [[f64; 2]; 3],
    /// Explicit tip in the second layer's local pixel coordinates, not its bounds.
    pub end_point: [f64; 2],
    pub driven_end: DrivenEnd,
    pub bend_direction: BendDirection,
    /// Control owned by this rig, not a layer. Vec2 in composition pixels;
    /// keyframe times are composition time. Expression/UI addressing is future work.
    pub target: Property,
    /// Original composition-space target. An untouched control preserves the authored
    /// pose, including the chosen elbow side. Missing in pre-evaluation project files.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bind_target: Option<[f64; 2]>,
}

fn default_label() -> effectcraft_color::Label {
    effectcraft_color::Label::Blue
}

impl Rig2d {
    /// Labels are editor metadata, just as on layers; do not invalidate pixels.
    pub(crate) fn same_pixels(&self, other: &Self) -> bool {
        let Self { id, label: _, layers, rest_joints, end_point, driven_end, bend_direction, target, bind_target } = self;
        *id == other.id
            && *layers == other.layers
            && *rest_joints == other.rest_joints
            && *end_point == other.end_point
            && *driven_end == other.driven_end
            && *bend_direction == other.bend_direction
            && *target == other.target
            && *bind_target == other.bind_target
    }

    /// Prepare a detached definition with two fresh ids. Does not parent layers,
    /// insert the rig, solve IK or change the pose. Call inside a project edit.
    pub fn new(
        project: &mut Project,
        layers: [LayerId; 2],
        rest_joints: [[f64; 2]; 3],
        end_point: [f64; 2],
        target_position: [f64; 2],
    ) -> Result<Self, ProjectError> {
        let next = project.next_id.checked_add(2).ok_or_else(|| ProjectError::Invalid("no ids available for a 2D rig".into()))?;
        let rig = Self {
            id: RigId(project.next_id),
            label: default_label(),
            layers,
            rest_joints,
            end_point,
            driven_end: DrivenEnd::Tip,
            bend_direction: BendDirection::Positive,
            target: Property::new(next - 1, "position", "Target Position", Value::Vec2(target_position)).with_ui(ParamUi::Point).spatial(),
            bind_target: Some(target_position),
        };
        rig.validate()?;
        project.next_id = next;
        Ok(rig)
    }

    fn validate(&self) -> Result<(), ProjectError> {
        let point = |p: &[f64; 2]| p.iter().all(|v| v.is_finite());
        let target = |v: &Value| matches!(v, Value::Vec2(p) if point(p));
        if self.id.0 == 0
            || self.id.0 == u64::MAX
            || self.target.uid == 0
            || self.target.uid == u64::MAX
            || self.layers[0] == self.layers[1]
            || !self.rest_joints.iter().all(point)
            || !point(&self.end_point)
            || self.bind_target.as_ref().is_some_and(|p| !point(p))
            || !target(&self.target.value)
            || self.target.keys.iter().any(|k| !target(&k.value))
        {
            return Err(ProjectError::Invalid(format!("invalid 2D rig {:?}: expected distinct layers, finite geometry and a Vec2 target", self.id)));
        }
        Ok(())
    }
}

/// Validate new identities globally, including collisions with existing document ids.
pub(crate) fn validate_project(project: &Project) -> Result<(), ProjectError> {
    if project.comps().all(|(_, comp)| comp.rigs2d.is_empty() && comp.timeline_groups.is_empty()) {
        return Ok(());
    }
    fn group_ids(group: &crate::PropGroup, ids: &mut BTreeSet<u64>, depth: usize) -> Result<(), ProjectError> {
        // Property readers already have their own nesting limits. Keep this walk bounded too.
        if depth > 128 {
            return Err(ProjectError::Invalid("property nesting exceeds 2D rig validation limit".into()));
        }
        ids.insert(group.uid);
        for node in &group.children {
            match node {
                crate::Node::Prop(p) => {
                    ids.insert(p.uid);
                }
                crate::Node::Group(g) => group_ids(g, ids, depth + 1)?,
            }
        }
        Ok(())
    }
    fn control_ids(controls: &[crate::essential::EgControl], ids: &mut BTreeSet<u64>, depth: usize) -> Result<(), ProjectError> {
        if depth > 128 {
            return Err(ProjectError::Invalid("control nesting exceeds 2D rig validation limit".into()));
        }
        for control in controls {
            ids.insert(control.id);
            if let crate::essential::EgKind::Group { children } = &control.kind {
                control_ids(children, ids, depth + 1)?;
            }
        }
        Ok(())
    }
    let mut ids = BTreeSet::new();
    for (id, item) in &project.items {
        ids.insert(id.0);
        if let Some(comp) = item.as_comp() {
            if let Some(essential) = &comp.essential {
                control_ids(&essential.controls, &mut ids, 0)?;
            }
            for layer in &comp.layers {
                ids.insert(layer.id.0);
                group_ids(&layer.props, &mut ids, 0)?;
            }
        }
    }
    for (_, comp) in project.comps() {
        crate::timeline_groups::validate(comp, &mut ids)?;
        for rig in &comp.rigs2d {
            rig.validate()?;
            if !ids.insert(rig.id.0) || !ids.insert(rig.target.uid) {
                return Err(ProjectError::Invalid(format!("duplicate 2D rig/control id in {:?}", rig.id)));
            }
            if rig.layers.iter().any(|id| comp.layer(*id).is_none()) {
                return Err(ProjectError::Invalid(format!("2D rig {:?} refers to a missing layer", rig.id)));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Comp, ItemId, ItemKind, LayerSource, build};
    use effectcraft_color::Label;
    use effectcraft_time::{FrameRate, Tick};

    fn fixture() -> (Project, ItemId) {
        let mut project = Project::default();
        let mut comp = Comp::new(640, 480, FrameRate::FPS_24, Tick::from_seconds_f64(2.0));
        for name in ["Upper", "Lower"] {
            let layer = build::layer(&mut project, &comp, name, LayerSource::Shape, (100, 20), None);
            comp.layers.push(layer);
        }
        let layers = [comp.layers[0].id, comp.layers[1].id];
        let mut rig = Rig2d::new(&mut project, layers, [[0.0, 0.0], [100.0, 0.0], [150.0, 50.0]], [50.0, 50.0], [150.0, 50.0]).unwrap();
        rig.driven_end = DrivenEnd::Root;
        rig.bend_direction = BendDirection::Negative;
        rig.target.set_animated(true, Tick::ZERO);
        rig.target.set_value_at(Tick::from_seconds_f64(1.0), Value::Vec2([200.0, 100.0]));
        comp.rigs2d.push(rig);
        let cid = project.add_item("Rig", Label::Sandstone, None, ItemKind::Comp(comp.into()));
        (project, cid)
    }

    #[test]
    fn file_roundtrip_preserves_rig_geometry_control_and_animation() {
        let (project, cid) = fixture();
        let loaded = Project::from_json(&project.to_file_json().unwrap()).unwrap();
        assert_eq!(project, loaded);
        let comp = loaded.comp(cid).unwrap();
        assert_eq!(comp.layers.len(), 2);
        assert_eq!(comp.rigs2d[0].target.value_at(Tick::from_seconds_f64(0.5)), Value::Vec2([175.0, 75.0]));
    }

    #[test]
    fn rig_labels_roundtrip_and_legacy_rigs_default_to_blue() {
        let (mut project, cid) = fixture();
        let rig = &mut project.comp_mut(cid).unwrap().rigs2d[0];
        assert_eq!(rig.label, Label::Blue);
        rig.label = Label::Fuchsia;
        let text = project.to_file_json().unwrap();
        let loaded = Project::from_json(&text).unwrap();
        assert_eq!(loaded, project);
        let mut changed = project.clone();
        changed.comp_mut(cid).unwrap().rigs2d[0].label = Label::Yellow;
        assert!(project.comp(cid).unwrap().same_pixels(changed.comp(cid).unwrap()));
        assert!(!project.same_content(&changed));
        let legacy = text.replace("\"label\": \"Fuchsia\",", "").replace("\"label\":\"Fuchsia\",", "");
        assert_ne!(legacy, text);
        let loaded = Project::from_json(&legacy).unwrap();
        assert_eq!(loaded.comp(cid).unwrap().rigs2d[0].label, Label::Blue);
    }

    #[test]
    fn older_projects_without_rigs_open_with_an_empty_collection() {
        let (mut project, cid) = fixture();
        project.comp_mut(cid).unwrap().rigs2d.clear();
        let text = project.to_file_json().unwrap();
        assert!(!text.contains("rigs2d"));
        let loaded = Project::from_json(&text).unwrap();
        assert!(loaded.comp(cid).unwrap().rigs2d.is_empty());
        assert_eq!(project, loaded);
    }

    #[test]
    fn loading_repairs_counter_and_new_rigs_get_distinct_ids() {
        let (mut project, cid) = fixture();
        let old = project.comp(cid).unwrap().rigs2d[0].clone();
        project.comp_mut(cid).unwrap().rigs2d[0].id = RigId(90_000);
        project.comp_mut(cid).unwrap().rigs2d[0].target.uid = 90_001;
        project.next_id = 1;
        let mut loaded = Project::from_json(&project.to_json()).unwrap();
        assert_eq!(loaded.next_id, 90_002);
        let new = Rig2d::new(&mut loaded, old.layers, old.rest_joints, old.end_point, [0.0; 2]).unwrap();
        assert_eq!(new.id, RigId(90_002));
        assert_eq!(new.target.uid, 90_003);
        assert_eq!(loaded.next_id, 90_004);
        assert_eq!(loaded.comp(cid).unwrap().rigs2d[0].id, RigId(90_000));
    }

    #[test]
    fn rig_edits_are_content_and_pixel_changes() {
        let (project, cid) = fixture();
        let mut edited = project.clone();
        edited.comp_mut(cid).unwrap().rigs2d[0].target.value = Value::Vec2([10.0, 20.0]);
        assert!(!project.same_content(&edited));
        assert!(!project.comp(cid).unwrap().same_pixels(edited.comp(cid).unwrap()));
        assert_ne!(project.comp(cid).unwrap().rigs2d, edited.comp(cid).unwrap().rigs2d);
    }

    #[test]
    fn imported_ids_and_layer_references_are_shifted_together() {
        let (mut project, cid) = fixture();
        let original = project.comp(cid).unwrap().rigs2d[0].clone();
        crate::essential::offset_ids(&mut project, 100_000);
        let loaded = Project::from_json(&project.to_file_json().unwrap()).unwrap();
        let rig = &loaded.comp(ItemId(cid.0 + 100_000)).unwrap().rigs2d[0];
        assert_eq!(rig.id.0, original.id.0 + 100_000);
        assert_eq!(rig.target.uid, original.target.uid + 100_000);
        assert_eq!(rig.layers, original.layers.map(|id| LayerId(id.0 + 100_000)));
        assert_eq!(rig.rest_joints, original.rest_joints);
        assert_eq!(rig.target.keys, original.target.keys);
    }

    #[test]
    fn invalid_rig_data_and_duplicate_ids_are_errors() {
        let (project, cid) = fixture();
        let mut duplicate = project.clone();
        let rig = duplicate.comp(cid).unwrap().rigs2d[0].clone();
        duplicate.comp_mut(cid).unwrap().rigs2d.push(rig);
        assert!(duplicate.to_file_json().is_err());
        let mut collision = project.clone();
        collision.comp_mut(cid).unwrap().rigs2d[0].id = RigId(cid.0);
        assert!(collision.to_file_json().is_err());
        let mut collision = project.clone();
        let uid = collision.comp(cid).unwrap().layers[0].props.uid;
        collision.comp_mut(cid).unwrap().rigs2d[0].target.uid = uid;
        assert!(collision.to_file_json().is_err());
        let mut collision = project.clone();
        let id = collision.comp(cid).unwrap().rigs2d[0].id.0;
        collision.comp_mut(cid).unwrap().essential = Some(crate::essential::EssentialGraphics {
            name: "Controls".into(),
            controls: vec![crate::essential::EgControl {
                id,
                name: "Comment".into(),
                kind: crate::essential::EgKind::Comment { text: String::new() },
                as_type: None,
            }],
        });
        assert!(collision.to_file_json().is_err());
        let mut missing = project.clone();
        missing.comp_mut(cid).unwrap().rigs2d[0].layers[1] = LayerId(999_999);
        assert!(missing.to_file_json().is_err());
        let mut invalid = project.clone();
        invalid.comp_mut(cid).unwrap().rigs2d[0].target.value = Value::Scalar(1.0);
        assert!(invalid.to_file_json().is_err());
        let mut invalid = project.clone();
        invalid.comp_mut(cid).unwrap().rigs2d[0].end_point[0] = f64::INFINITY;
        assert!(invalid.to_file_json().is_err());
        let mut exhausted = project;
        exhausted.next_id = u64::MAX;
        let next = exhausted.next_id;
        assert!(Rig2d::new(&mut exhausted, [LayerId(1), LayerId(2)], [[0.0; 2]; 3], [0.0; 2], [0.0; 2]).is_err());
        assert_eq!(exhausted.next_id, next);
    }
}
