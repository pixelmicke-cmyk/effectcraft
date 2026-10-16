//! Flat, non-rendering timeline organization. Control tracks follow their owner layer.
use crate::{ItemKind, LayerId, Project, ProjectError};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimelineGroupId(pub u64);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TimelineGroup {
    pub id: TimelineGroupId,
    pub name: String,
    pub label: effectcraft_color::Label,
    /// Membership only. Display and rendering retain Comp::layers order.
    pub layers: Vec<LayerId>,
    #[serde(default)]
    pub collapsed: bool,
}

pub(crate) fn validate(comp: &crate::Comp, ids: &mut BTreeSet<u64>) -> Result<(), ProjectError> {
    let mut members = BTreeSet::new();
    for group in &comp.timeline_groups {
        if group.id.0 == 0
            || group.id.0 == u64::MAX
            || !ids.insert(group.id.0)
            || group.name.trim().is_empty()
            || group.name.chars().count() > 256
            || group.layers.iter().any(|l| comp.layer(*l).is_none() || !members.insert(*l))
        {
            return Err(ProjectError::Invalid("invalid timeline group id, name or membership".into()));
        }
    }
    Ok(())
}

/// A layer deletion also removes its organizational references, in the same undo edit.
/// Empty groups remain editable. On file load the caller records these repairs.
pub fn prune(project: &mut Project) -> Vec<String> {
    let mut notes = vec![];
    for item in project.items.values_mut() {
        if let ItemKind::Comp(comp) = &mut item.kind {
            if !comp.timeline_groups.iter().any(|g| g.layers.iter().any(|l| comp.layer(*l).is_none())) {
                continue;
            }
            let comp = std::sync::Arc::make_mut(comp);
            let layers: BTreeSet<_> = comp.layers.iter().map(|l| l.id).collect();
            for group in &mut comp.timeline_groups {
                group.layers.retain(|l| layers.contains(l));
            }
            notes.push(format!("Removed missing timeline group members in {}", item.name));
        }
    }
    notes
}
