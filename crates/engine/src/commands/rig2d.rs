use effectcraft_geom::{ik2d::solve_two_bone, vec2, vec3};
use effectcraft_keyframe::Value as KV;
use effectcraft_render::EvalCtx;
use serde_json::{Value, json};

use super::{CommandSpec, bad, has_layers, layer_mut, layers_p};
use crate::{Result, Session, cmd};

const CONNECT: &str = "rig.connect2d";

fn set_label(s: &mut Session, p: &Value) -> Result<Value> {
    const COMMAND: &str = "rig.setLabel2d";
    let cid = super::comp_id(s, p)?;
    let id = p.get("rig").and_then(Value::as_u64).ok_or_else(|| bad(COMMAND, "missing rig id"))?;
    let name = p.get("label").and_then(Value::as_str).ok_or_else(|| bad(COMMAND, "missing label name"))?;
    let label = s.prefs.label_from_name(name).ok_or_else(|| bad(COMMAND, format!("unknown label `{name}`")))?;
    if !s.project.comp(cid).is_some_and(|c| c.rigs2d.iter().any(|r| r.id.0 == id)) {
        return Err(bad(COMMAND, "rig not found in composition"));
    }
    s.edit("Set IK Label", None, |project, _| {
        let rig = project.comp_mut(cid).and_then(|c| c.rigs2d.iter_mut().find(|r| r.id.0 == id)).ok_or_else(|| bad(COMMAND, "rig not found"))?;
        rig.label = label;
        Ok(())
    })?;
    Ok(json!({"rig":id,"label":label.name()}))
}

fn select_control(s: &mut Session, p: &Value) -> Result<Value> {
    let cid = super::comp_id(s, p)?;
    let id = match p.get("rig") {
        Some(Value::Null) => None,
        Some(v) => Some(effectcraft_project::rig2d::RigId(v.as_u64().ok_or_else(|| bad("rig.select2d", "rig must be an id or null"))?)),
        None => return Err(bad("rig.select2d", "missing rig")),
    };
    if let Some(id) = id {
        let comp = s.project.comp(cid).ok_or_else(|| bad("rig.select2d", "composition not found"))?;
        if s.active_comp_id() != Some(cid) || !comp.rigs2d.iter().any(|r| r.id == id) {
            return Err(bad("rig.select2d", "rig must belong to the active composition"));
        }
    }
    s.state.selected_rig2d = id;
    if p.get("timeline").and_then(Value::as_bool) == Some(true) {
        s.state.selected_layers.clear();
        s.state.selected_props.clear();
        s.state.selected_keys.clear();
        s.state.selected_vertices.clear();
    }
    Ok(json!({"rig": id.map(|id| id.0)}))
}

fn set_target(s: &mut Session, p: &Value) -> Result<Value> {
    const COMMAND: &str = "rig.setTarget2d";
    let cid = super::comp_id(s, p)?;
    let id = p.get("rig").and_then(Value::as_u64).ok_or_else(|| bad(COMMAND, "missing rig id"))?;
    let position =
        p.get("position").and_then(Value::as_array).filter(|p| p.len() == 2).ok_or_else(|| bad(COMMAND, "position must be [x, y] in composition pixels"))?;
    let number =
        |i| position.get(i).and_then(Value::as_f64).filter(|n| n.is_finite()).ok_or_else(|| bad(COMMAND, "position coordinates must be finite numbers"));
    let position = [number(0)?, number(1)?];
    let time = s.time_of(cid);
    let comp = s.project.comp(cid).ok_or_else(|| bad(COMMAND, "composition not found"))?;
    if comp.rigs2d.iter().filter(|r| r.id.0 == id).count() != 1 {
        return Err(bad(COMMAND, "expected one rig with this id in the composition"));
    }
    let rig = comp.rigs2d.iter().find(|r| r.id.0 == id).ok_or_else(|| bad(COMMAND, "rig not found"))?;
    for layer in rig.layers {
        let layer = comp.layer(layer).ok_or_else(|| bad(COMMAND, "missing rig layer"))?;
        if layer.switches.locked {
            return Err(bad(COMMAND, "unlock the rig's layers before moving its control"));
        }
    }
    // Prepare and validate on a snapshot: a malformed rig cannot leave a partial edit.
    let mut prepared = (*s.project).clone();
    let c = prepared.comp_mut(cid).ok_or_else(|| bad(COMMAND, "composition not found"))?;
    let r = c.rigs2d.iter_mut().find(|r| r.id.0 == id).ok_or_else(|| bad(COMMAND, "rig not found"))?;
    let before: Vec<_> = r.target.keys.iter().map(|k| k.time).collect();
    r.target.set_value_at(time, KV::Vec2(position));
    super::prop::new_key_defaults(&mut r.target, &before, s.prefs.general.default_spatial_linear);
    let c = prepared.comp(cid).ok_or_else(|| bad(COMMAND, "composition not found"))?;
    let r = c.rigs2d.iter().find(|r| r.id.0 == id).ok_or_else(|| bad(COMMAND, "rig not found"))?;
    let ctx = EvalCtx { expr: s.expr.as_deref(), footage: Some(s.footage.as_ref()), ..EvalCtx::new(&prepared, cid, c, time) };
    ctx.rig2d_pose(r).map_err(|e| bad(COMMAND, e.to_string()))?;
    s.edit("Move IK Target", super::merge_p(p), |project, _| {
        *project = prepared;
        Ok(())
    })?;
    Ok(json!({"rig": id, "position": position}))
}

/// Ancestors use local_matrix, excluding source pixel-aspect stretch, just as
/// world_matrix does. Validate before evaluating a loaded hierarchy (renderer cap: 64).
fn parent_space(ctx: &EvalCtx<'_>, layer: &effectcraft_project::Layer, require_static: bool) -> Result<effectcraft_geom::Mat4> {
    use effectcraft_geom::Mat4;
    let mut seen = std::collections::HashSet::new();
    let mut current = Some(layer.id);
    let mut parent = Mat4::IDENTITY;
    while let Some(id) = current {
        if seen.len() >= 64 || !seen.insert(id) {
            return Err(bad(CONNECT, "cyclic or excessively deep layer hierarchy"));
        }
        let item = ctx.comp.layer(id).ok_or_else(|| bad(CONNECT, "missing ancestor layer"))?;
        if item.is_3d() || item.is_camera() || item.is_light() {
            return Err(bad(CONNECT, "only 2D layers and 2D ancestors are supported"));
        }
        let tr = item.transform().ok_or_else(|| bad(CONNECT, "layer has no transform"))?;
        for name in ["anchor", "scale", "rotation"] {
            if tr.get(name).is_none() {
                return Err(bad(CONNECT, format!("layer has no {name}")));
            }
        }
        if tr.get("position").is_none() {
            return Err(bad(CONNECT, "layer has no position"));
        }
        if require_static
            && (item.auto_orient != effectcraft_project::AutoOrient::Off
                || tr
                    .props()
                    .filter(|p| matches!(p.match_id.as_str(), "anchor" | "position" | "positionX" | "positionY" | "positionZ" | "rotation" | "scale"))
                    .any(|p| !p.keys.is_empty() || p.expr.is_some()))
        {
            return Err(bad(CONNECT, "automatic parenting requires static transforms without expressions or auto-orientation; parent the chain first"));
        }
        let m = ctx.local_matrix(item);
        if !m.0.iter().flatten().all(|v| v.is_finite()) || m.inverse().is_none() {
            return Err(bad(CONNECT, "non-finite or singular layer transform"));
        }
        if id != layer.id {
            parent = m * parent;
        }
        current = item.parent;
    }
    Ok(parent)
}

/// Persist a rig without solving it. All preparation, including Parent & Link's
/// compensation, happens on a private snapshot; only the final edit touches history.
fn connect_ik(s: &mut Session, p: &Value) -> Result<Value> {
    use super::resolve_layer;
    use effectcraft_geom::Mat4;
    use effectcraft_project::rig2d::{BendDirection, DrivenEnd, Rig2d};
    if p.get("layers").is_some_and(|v| !v.is_array()) {
        return Err(bad(CONNECT, "layers must be an array"));
    }
    let (cid, ids) = layers_p(s, p)?;
    let [a, b] = ids.as_slice() else {
        return Err(bad(CONNECT, "select exactly two distinct layers"));
    };
    if a == b {
        return Err(bad(CONNECT, "select exactly two distinct layers"));
    }
    let comp = s.project.comp(cid).ok_or_else(|| bad(CONNECT, "composition not found"))?;
    let first = comp.layer(*a).ok_or_else(|| bad(CONNECT, "layer not found"))?;
    let second = comp.layer(*b).ok_or_else(|| bad(CONNECT, "layer not found"))?;
    let inferred = match (first.parent == Some(*b), second.parent == Some(*a)) {
        (true, false) => Some(*b),
        (false, true) => Some(*a),
        (true, true) => return Err(bad(CONNECT, "cyclic layer hierarchy")),
        (false, false) => None,
    };
    let explicit = p.get("root").map(|v| resolve_layer(comp, v).ok_or_else(|| bad(CONNECT, "root layer not found"))).transpose()?;
    let root_id = match (explicit, inferred) {
        (Some(root), inferred) if ids.contains(&root) && inferred.is_none_or(|id| id == root) => root,
        (None, Some(root)) => root,
        _ => return Err(bad(CONNECT, "ambiguous or conflicting chain order; provide root from the selected layers")),
    };
    let tip_id = if root_id == *a { *b } else { *a };
    let upper = comp.layer(root_id).ok_or_else(|| bad(CONNECT, "root layer not found"))?;
    let lower = comp.layer(tip_id).ok_or_else(|| bad(CONNECT, "tip layer not found"))?;
    if upper.switches.locked || lower.switches.locked {
        return Err(bad(CONNECT, "unlock both layers before connecting"));
    }
    if comp.rigs2d.iter().any(|rig| rig.layers.iter().any(|id| ids.contains(id))) {
        return Err(bad(CONNECT, "a selected layer already belongs to a 2D rig"));
    }
    let endpoint = p
        .get("end_point")
        .and_then(Value::as_array)
        .filter(|v| v.len() == 2)
        .ok_or_else(|| bad(CONNECT, "end_point must be [x, y] in the tip layer's local coordinates"))?;
    let number = |index| {
        endpoint.get(index).and_then(Value::as_f64).filter(|v| v.is_finite()).ok_or_else(|| bad(CONNECT, "end_point coordinates must be finite numbers"))
    };
    let end_point = [number(0)?, number(1)?];
    let bend = match p.get("bend_direction") {
        None => BendDirection::Positive,
        Some(v) if v.as_f64() == Some(1.0) => BendDirection::Positive,
        Some(v) if v.as_f64() == Some(-1.0) => BendDirection::Negative,
        _ => return Err(bad(CONNECT, "bend_direction must be 1 or -1")),
    };
    let driven = match p.get("driven_end") {
        None => DrivenEnd::Tip,
        Some(v) if v.as_str() == Some("tip") => DrivenEnd::Tip,
        Some(v) if v.as_str() == Some("root") => DrivenEnd::Root,
        _ => return Err(bad(CONNECT, "driven_end must be root or tip")),
    };
    let reparent = lower.parent != Some(root_id);
    let ctx = EvalCtx { expr: s.expr.as_deref(), footage: Some(s.footage.as_ref()), ..EvalCtx::new(&s.project, cid, comp, s.time_of(cid)) };
    let parent = parent_space(&ctx, upper, reparent)?;
    parent_space(&ctx, lower, reparent)?;
    let inv = parent.inverse().filter(|m| m.0.iter().flatten().all(|v| v.is_finite())).ok_or_else(|| bad(CONNECT, "root parent space is not invertible"))?;
    let anchor = |layer: &effectcraft_project::Layer| -> Result<_> {
        let tr = layer.transform().ok_or_else(|| bad(CONNECT, "missing transform"))?;
        let a = ctx.v3(layer, tr, "anchor", [0.0; 3]);
        Ok(ctx.world_matrix(layer).apply(vec3(a[0], a[1], 0.0)))
    };
    let shoulder = anchor(upper)?;
    let elbow = anchor(lower)?;
    let tip = ctx.world_matrix(lower).apply(vec3(end_point[0], end_point[1], 0.0));
    let rest = [shoulder, elbow, tip].map(|p| {
        let p = inv.apply(p);
        [p.x, p.y]
    });
    if !rest.iter().flatten().all(|v| v.is_finite()) || ![shoulder, elbow, tip].iter().all(|p| p.x.is_finite() && p.y.is_finite()) {
        return Err(bad(CONNECT, "non-finite rig geometry"));
    }
    for pair in rest.windows(2) {
        let [a, b] = pair else {
            return Err(bad(CONNECT, "invalid joint geometry"));
        };
        let length = (b[0] - a[0]).hypot(b[1] - a[1]);
        if !length.is_finite() || length <= 0.0 {
            return Err(bad(CONNECT, "both rig segments must have finite, non-zero length"));
        }
    }
    let before = ctx.world_matrix(lower);
    let mut prepared = (*s.project).clone();
    if reparent {
        super::layer::check_parent_chain(comp, &[tip_id], Some(root_id), 64, 1_048_576)?;
        let fixes = super::layer::parent_fixes(s, cid, &[tip_id], Some(root_id));
        let fix = fixes.first().ok_or_else(|| bad(CONNECT, "cannot compensate parenting"))?;
        let layer = layer_mut(&mut prepared, cid, tip_id)?;
        layer.parent = Some(root_id);
        fix.apply(layer);
        let after_comp = prepared.comp(cid).ok_or_else(|| bad(CONNECT, "composition not found"))?;
        let after_layer = after_comp.layer(tip_id).ok_or_else(|| bad(CONNECT, "tip layer not found"))?;
        let after_ctx = EvalCtx { project: &prepared, comp: after_comp, ..ctx };
        let after = after_ctx.world_matrix(after_layer);
        let same = |a: Mat4, b: Mat4| {
            a.0.iter().flatten().zip(b.0.iter().flatten()).all(|(a, b)| {
                let scale = a.abs().max(b.abs()).max(1.0);
                a.is_finite() && b.is_finite() && (a / scale - b / scale).abs() <= 1e-10
            })
        };
        if !same(before, after) {
            return Err(bad(CONNECT, "parenting cannot preserve this pose (unsupported transform or shear)"));
        }
    }
    prepared.fix_next_id();
    let target = if driven == DrivenEnd::Tip { tip } else { shoulder };
    let mut rig = Rig2d::new(&mut prepared, [root_id, tip_id], rest, end_point, [target.x, target.y])?;
    rig.bend_direction = bend;
    rig.driven_end = driven;
    let rig_id = rig.id;
    let control = rig.target.uid;
    prepared.comp_mut(cid).ok_or_else(|| bad(CONNECT, "composition not found"))?.rigs2d.push(rig);
    s.edit("Connect 2D Rig", None, |project, _| {
        *project = prepared;
        Ok(())
    })?;
    Ok(json!({"rig": rig_id.0, "control": control, "layers": [root_id.0, tip_id.0]}))
}

fn solve_ik(s: &mut Session, p: &Value) -> Result<Value> {
    let (cid, mut layers) = layers_p(s, p)?;

    if layers.len() == 2 {
        let comp = s.project.comp(cid).ok_or_else(|| bad("rig.solveIk2d", "composition not found"))?;

        let first_parent = comp.layer(layers[0]).and_then(|l| l.parent);
        let second_parent = comp.layer(layers[1]).and_then(|l| l.parent);

        if first_parent == Some(layers[1]) {
            layers.swap(0, 1);
        } else if second_parent != Some(layers[0]) {
            return Err(bad("rig.solveIk2d", "the second layer must be parented to the first"));
        }
    }

    if layers.len() != 2 {
        return Err(bad("rig.solveIk2d", "select exactly two layers"));
    }

    let x = p.get("x").and_then(Value::as_f64).ok_or_else(|| bad("rig.solveIk2d", "missing x"))?;
    let y = p.get("y").and_then(Value::as_f64).ok_or_else(|| bad("rig.solveIk2d", "missing y"))?;

    let comp = s.project.comp(cid).ok_or_else(|| bad("rig.solveIk2d", "composition not found"))?;

    let upper = comp.layer(layers[0]).ok_or_else(|| bad("rig.solveIk2d", "upper arm not found"))?;

    let ctx = EvalCtx::new(&s.project, cid, comp, s.time());

    let transform = upper.transform().ok_or_else(|| bad("rig.solveIk2d", "upper arm has no transform"))?;

    let anchor = ctx.v3(upper, transform, "anchor", [0.0; 3]);

    let shoulder = ctx.world_matrix(upper).apply(vec3(anchor[0], anchor[1], anchor[2]));

    let lower = comp.layer(layers[1]).ok_or_else(|| bad("rig.solveIk2d", "lower arm not found"))?;

    let lower_transform = lower.transform().ok_or_else(|| bad("rig.solveIk2d", "lower arm has no transform"))?;

    let lower_anchor = ctx.v3(lower, lower_transform, "anchor", [0.0; 3]);

    let elbow = ctx.world_matrix(lower).apply(vec3(lower_anchor[0], lower_anchor[1], lower_anchor[2]));

    let bounds = effectcraft_render::content_bounds(&ctx, lower).ok_or_else(|| bad("rig.solveIk2d", "lower arm has no visible bounds"))?;

    let hand = ctx.world_matrix(lower).apply(vec3(bounds[2], lower_anchor[1], lower_anchor[2]));

    let second_length = ((hand.x - elbow.x).powi(2) + (hand.y - elbow.y).powi(2)).sqrt();
    let first_length = ((elbow.x - shoulder.x).powi(2) + (elbow.y - shoulder.y).powi(2)).sqrt();
    let root = vec2(shoulder.x, shoulder.y);
    let target = vec2(x, y);

    let result = solve_two_bone(root, target, first_length, second_length, 1.0).ok_or_else(|| bad("rig.solveIk2d", "invalid IK parameters"))?;
    let current_first_angle = (elbow.y - shoulder.y).atan2(elbow.x - shoulder.x).to_degrees();

    let current_second_angle = (hand.y - elbow.y).atan2(hand.x - elbow.x).to_degrees();

    let first_delta = result.first_angle - current_first_angle;

    let second_delta = result.second_angle - current_second_angle - first_delta;

    let t = s.time();

    s.edit("Solve 2D IK", None, |proj, _| {
        // Överarm
        let first = layer_mut(proj, cid, layers[0])?;
        let lt = first.layer_time(t);

        let rotation = first.props.prop_mut("transform/rotation").ok_or_else(|| bad("rig.solveIk2d", "first layer has no rotation"))?;

        let current = rotation.value_at(lt).as_f64();
        rotation.set_value_at(lt, KV::Scalar(current + first_delta));

        // Underarm
        let second = layer_mut(proj, cid, layers[1])?;
        let lt = second.layer_time(t);

        let rotation = second.props.prop_mut("transform/rotation").ok_or_else(|| bad("rig.solveIk2d", "second layer has no rotation"))?;

        let current = rotation.value_at(lt).as_f64();
        rotation.set_value_at(lt, KV::Scalar(current + second_delta));

        Ok(())
    })?;

    Ok(json!({
        "first_angle": result.first_angle,
        "second_angle": result.second_angle
    }))
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!("rig.setLabel2d", "Set IK Label", [], None, "{rig: id, label: name}", super::has_comp, set_label),
        cmd!(
            "rig.select2d",
            "Select IK Control",
            [],
            None,
            "{rig: id|null, timeline?: bool (clear layer/property/key selection)}",
            super::has_comp,
            select_control
        ),
        cmd!("rig.setTarget2d", "Move IK Target", [], None, "{rig: id, position: [x,y], merge?}", super::has_comp, set_target),
        cmd!(
            "rig.connect2d",
            "Connect 2D Rig",
            [],
            None,
            "{layers?, root?: layer, end_point: [x,y] (tip-local), bend_direction?: 1|-1, driven_end?: root|tip}",
            super::has_comp,
            connect_ik
        ),
        cmd!("rig.solveIk2d", "Solve 2D IK", [], None, "{x, y, layers?}", has_layers, solve_ik),
    ]
}
