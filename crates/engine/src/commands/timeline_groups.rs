use super::{CommandSpec, bad, comp_id, has_comp, layers_p};
use crate::{Result, Session, cmd};
use effectcraft_project::timeline_groups::{TimelineGroup, TimelineGroupId};
use serde_json::{Value, json};

fn change(s: &mut Session, p: &Value, op: &str) -> Result<Value> {
    const COMMAND: &str = "timeline.group";
    let cid = comp_id(s, p)?;
    let layers = if matches!(op, "create" | "add" | "remove") {
        if p.get("layers").is_some_and(|v| !v.is_array()) {
            return Err(bad(COMMAND, "layers must be an array"));
        }
        let (owner, mut layers) = layers_p(s, p)?;
        layers.sort_unstable();
        layers.dedup();
        if owner != cid || layers.is_empty() {
            return Err(bad(COMMAND, "select at least one layer"));
        }
        layers
    } else {
        vec![]
    };
    let id = p.get("group").map(|v| v.as_u64().ok_or_else(|| bad(COMMAND, "group must be an id"))).transpose()?;
    if !matches!(op, "create" | "remove") && id.is_none() {
        return Err(bad(COMMAND, "missing group id"));
    }
    if let Some(id) = id {
        if !s.project.comp(cid).is_some_and(|c| c.timeline_groups.iter().any(|g| g.id.0 == id)) {
            return Err(bad(COMMAND, "group not found"));
        }
    }
    let name = match p.get("name") {
        Some(v) => v.as_str().ok_or_else(|| bad(COMMAND, "name must be text"))?,
        None if op == "rename" => return Err(bad(COMMAND, "missing group name")),
        None => "Group",
    }
    .trim();
    if matches!(op, "create" | "rename") && (name.is_empty() || name.chars().count() > 256) {
        return Err(bad(COMMAND, "group name must contain 1–256 characters"));
    }
    let name = name.to_string();
    let label = if op == "label" {
        let name = p.get("label").and_then(Value::as_str).ok_or_else(|| bad(COMMAND, "missing label"))?;
        Some(s.prefs.label_from_name(name).ok_or_else(|| bad(COMMAND, "unknown palette label"))?)
    } else {
        None
    };
    let collapsed =
        if op == "collapse" { Some(p.get("collapsed").and_then(Value::as_bool).ok_or_else(|| bad(COMMAND, "collapsed must be true or false"))?) } else { None };
    s.edit("Edit Timeline Group", None, |project, _| {
        let id = if op == "create" {
            let id = project.next_id;
            project.next_id = id.checked_add(1).ok_or_else(|| bad(COMMAND, "no group ids available"))?;
            id
        } else {
            id.unwrap_or(0)
        };
        let comp = project.comp_mut(cid).ok_or_else(|| bad(COMMAND, "composition not found"))?;
        match op {
            "create" | "add" => {
                // Moving into a group transfers membership, never render order or parenting.
                for group in &mut comp.timeline_groups {
                    group.layers.retain(|l| !layers.contains(l));
                }
                if op == "create" {
                    comp.timeline_groups.push(TimelineGroup { id: TimelineGroupId(id), name, label: effectcraft_color::Label::Blue, layers, collapsed: false });
                } else {
                    let group = comp.timeline_groups.iter_mut().find(|g| g.id.0 == id).ok_or_else(|| bad(COMMAND, "group not found"))?;
                    group.layers.extend(layers);
                }
            }
            "remove" => {
                for group in &mut comp.timeline_groups {
                    if id == 0 || group.id.0 == id {
                        group.layers.retain(|l| !layers.contains(l));
                    }
                }
            }
            "dissolve" => comp.timeline_groups.retain(|g| g.id.0 != id),
            _ => {
                let group = comp.timeline_groups.iter_mut().find(|g| g.id.0 == id).ok_or_else(|| bad(COMMAND, "group not found"))?;
                if op == "rename" {
                    group.name = name;
                }
                if let Some(label) = label {
                    group.label = label;
                }
                if let Some(collapsed) = collapsed {
                    group.collapsed = collapsed;
                }
            }
        }
        Ok(json!({"group":id}))
    })
}
fn create(s: &mut Session, p: &Value) -> Result<Value> {
    change(s, p, "create")
}
fn add(s: &mut Session, p: &Value) -> Result<Value> {
    change(s, p, "add")
}
fn remove(s: &mut Session, p: &Value) -> Result<Value> {
    change(s, p, "remove")
}
fn dissolve(s: &mut Session, p: &Value) -> Result<Value> {
    change(s, p, "dissolve")
}
fn rename(s: &mut Session, p: &Value) -> Result<Value> {
    change(s, p, "rename")
}
fn label(s: &mut Session, p: &Value) -> Result<Value> {
    change(s, p, "label")
}
fn collapse(s: &mut Session, p: &Value) -> Result<Value> {
    change(s, p, "collapse")
}
pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!("timeline.group.create", "Create Timeline Group", [], None, "{layers?,name?}", has_comp, create),
        cmd!("timeline.group.add", "Add Layers to Group", [], None, "{group,layers?}", has_comp, add),
        cmd!("timeline.group.remove", "Remove Layers from Group", [], None, "{group?,layers?}", has_comp, remove),
        cmd!("timeline.group.dissolve", "Dissolve Timeline Group", [], None, "{group}", has_comp, dissolve),
        cmd!("timeline.group.rename", "Rename Timeline Group", [], None, "{group,name}", has_comp, rename),
        cmd!("timeline.group.label", "Set Timeline Group Label", [], None, "{group,label}", has_comp, label),
        cmd!("timeline.group.collapse", "Collapse Timeline Group", [], None, "{group,collapsed}", has_comp, collapse),
    ]
}
