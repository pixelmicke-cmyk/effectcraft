//! Pure, bounded two-bone transform evaluation. Rig inputs are read through the
//! authored transform path; solving never calls world_matrix or another rig.

use effectcraft_geom::{Mat4, Vec2, Vec3, ik2d::solve_two_bone, vec2, vec3};
use effectcraft_project::{
    AutoOrient, Layer, Value,
    rig2d::{BendDirection, DrivenEnd, Rig2d, RigId},
};

use crate::EvalCtx;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RigError {
    pub rig: RigId,
    pub message: String,
}

impl std::fmt::Display for RigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "2D IK rig {}: {} (authored pose retained)", self.rig.0, self.message)
    }
}
impl std::error::Error for RigError {}

/// Evaluated parent-space transforms, in the rig's root-to-tip order.
#[derive(Clone, Copy, Debug)]
pub struct RigPose {
    pub upper: Mat4,
    pub lower: Mat4,
}

fn finite(m: Mat4) -> bool {
    m.0.iter().flatten().all(|v| v.is_finite())
}
fn point(p: Vec3) -> Vec2 {
    vec2(p.x, p.y)
}
fn angle(v: Vec2) -> f64 {
    v.y.atan2(v.x).to_degrees()
}
fn length(v: Vec2) -> f64 {
    v.x.hypot(v.y)
}
fn translation(p: Vec2) -> Mat4 {
    Mat4::translate(vec3(p.x, p.y, 0.0))
}

fn close_matrix(a: Mat4, b: Mat4) -> bool {
    a.0.iter().flatten().zip(b.0.iter().flatten()).all(|(a, b)| {
        let scale = a.abs().max(b.abs()).max(1.0);
        a.is_finite() && b.is_finite() && (a / scale - b / scale).abs() <= 1e-8
    })
}

impl EvalCtx<'_> {
    /// Evaluate one saved rig, or return an actionable unsupported/corrupt-input error.
    pub fn rig2d_pose(&self, rig: &Rig2d) -> Result<RigPose, RigError> {
        let error = |message: &str| RigError { rig: rig.id, message: message.into() };
        let upper = self.layer(rig.layers[0]).ok_or_else(|| error("missing upper layer"))?;
        let lower = self.layer(rig.layers[1]).ok_or_else(|| error("missing lower layer"))?;
        if upper.id == lower.id || lower.parent != Some(upper.id) {
            return Err(error("expected two distinct layers with the lower parented to the upper"));
        }
        // Validate every dependency before calling any authored evaluator. Expressions
        // and auto-orient may themselves request world transforms, so they are excluded.
        for start in [upper, lower] {
            let mut current = Some(start.id);
            let mut seen = std::collections::HashSet::new();
            while let Some(id) = current {
                if seen.len() >= 64 || !seen.insert(id) {
                    return Err(error("cyclic or excessively deep parent hierarchy"));
                }
                let layer = self.layer(id).ok_or_else(|| error("missing parent layer"))?;
                if self.comp.rigs2d.iter().filter(|r| r.layers.contains(&id)).count() != usize::from(rig.layers.contains(&id)) {
                    return Err(error("overlapping rigs or rigged ancestors are not supported"));
                }
                if layer.is_3d() || layer.is_camera() || layer.is_light() || layer.auto_orient != AutoOrient::Off {
                    return Err(error("only 2D transforms without auto-orientation are supported"));
                }
                let tr = layer.transform().ok_or_else(|| error("missing transform"))?;
                for name in ["anchor", "position", "rotation", "scale"] {
                    if tr.get(name).is_none() {
                        return Err(error("incomplete transform"));
                    }
                }
                if tr
                    .props()
                    .filter(|p| matches!(p.match_id.as_str(), "anchor" | "position" | "positionX" | "positionY" | "positionZ" | "rotation" | "scale"))
                    .any(|p| p.has_expression())
                    || rig.target.has_expression()
                {
                    return Err(error("expressions on IK targets or ancestor transforms are not supported"));
                }
                if tr
                    .props()
                    .filter(|p| matches!(p.match_id.as_str(), "scale" | "anchor") || (rig.layers.contains(&id) && p.match_id.starts_with("position")))
                    .any(|p| !p.keys.is_empty())
                {
                    return Err(error("animated scales, anchors or bone positions are not supported; animate the IK target or the body's position/rotation"));
                }
                let m = self.authored_local_matrix_par(layer, 1.0);
                if !finite(m) || m.inverse().is_none() {
                    return Err(error("non-finite or singular transform"));
                }
                current = layer.parent;
            }
        }
        let authored_upper = self.authored_local_matrix_par(upper, 1.0);
        let authored_lower = self.authored_local_matrix_par(lower, 1.0);
        let mut parent = Mat4::IDENTITY;
        let mut current = upper.parent;
        // Already validated for cycles/depth above; keep a separate hard work bound.
        for _ in 0..64 {
            let Some(id) = current else { break };
            let layer = self.layer(id).ok_or_else(|| error("missing parent"))?;
            parent = self.authored_local_matrix_par(layer, 1.0) * parent;
            current = layer.parent;
        }
        let inv_parent = parent.inverse().filter(|m| finite(*m)).ok_or_else(|| error("parent space is not invertible"))?;
        let anchor = |layer: &Layer| -> Result<Vec3, RigError> {
            let tr = layer.transform().ok_or_else(|| error("missing anchor"))?;
            let a = self.v3(layer, tr, "anchor", [0.0; 3]);
            Ok(vec3(a[0], a[1], 0.0))
        };
        let root = point(authored_upper.apply(anchor(upper)?));
        let elbow = point((authored_upper * authored_lower).apply(anchor(lower)?));
        let tip = point((authored_upper * authored_lower).apply(vec3(rig.end_point[0], rig.end_point[1], 0.0)));
        let Value::Vec2(target) = rig.target.value_at(self.time) else {
            return Err(error("target must be a Vec2"));
        };
        if !target.iter().all(|v| v.is_finite()) {
            return Err(error("target must be finite"));
        }
        let bind = rig.bind_target.unwrap_or_else(|| {
            let p = parent.apply(if rig.driven_end == DrivenEnd::Tip { vec3(tip.x, tip.y, 0.0) } else { vec3(root.x, root.y, 0.0) });
            [p.x, p.y]
        });
        let untouched = target == bind && rig.target.value == Value::Vec2(bind) && rig.target.keys.iter().all(|k| k.value == Value::Vec2(bind));
        if untouched {
            return Ok(RigPose { upper: authored_upper, lower: authored_lower });
        }
        // Solve in the saved root-parent metric, using the lengths of transformed
        // anchor/end-point vectors, never the magnitudes of local Scale properties.
        // Each complete bone matrix receives a rigid correction in that space.
        // The lower LOCAL matrix may then contain compensating shear; decomposing
        // it to an angle and Scale would lose the original shape under anisotropy.
        // External ancestors must remain similarities so this metric is also
        // Euclidean in composition space (up to one common scale).
        for layer in [upper, lower] {
            if self.par_ratio(layer) != 1.0 {
                return Err(error("non-square source pixels are not supported for active IK"));
            }
        }
        for start in [upper, lower] {
            let mut current = Some(start.id);
            for _ in 0..64 {
                let Some(id) = current else { break };
                let layer = self.layer(id).ok_or_else(|| error("missing layer"))?;
                let tr = layer.transform().ok_or_else(|| error("missing scale"))?;
                let s = self.v3(layer, tr, "scale", [100.0; 3]);
                if !s[0].is_finite() || !s[1].is_finite() || s[0] <= 0.0 || s[1] <= 0.0 {
                    return Err(error(&format!("Cannot move IK target: \"{}\" needs finite positive Scale X/Y (reflections are not supported).", layer.name)));
                }
                if !rig.layers.contains(&id) && (s[0] / s[1] - 1.0).abs() > 1e-10 {
                    return Err(error(&format!(
                        "Cannot move IK target: ancestor \"{}\" has unequal Scale X/Y. Nonuniform scale is supported on the two rig layers only.",
                        layer.name
                    )));
                }
                current = layer.parent;
            }
        }
        let [a, b, c] = rig.rest_joints.map(|p| vec2(p[0], p[1]));
        let l1 = length(b - a);
        let l2 = length(c - b);
        if !l1.is_finite() || !l2.is_finite() || l1 <= 0.0 || l2 <= 0.0 {
            return Err(error("invalid saved bone lengths"));
        }
        for (current, saved) in [(length(elbow - root), l1), (length(tip - elbow), l2)] {
            if !current.is_finite() || (current / saved - 1.0).abs() > 1e-8 {
                return Err(error("bone geometry changed after Connect; reconnect the rig"));
            }
        }
        let target = point(inv_parent.apply(vec3(target[0], target[1], 0.0)));
        let side = if rig.bend_direction == BendDirection::Positive { 1.0 } else { -1.0 };
        let solve = if rig.driven_end == DrivenEnd::Tip {
            solve_two_bone(root, target, l1, l2, side).map(|r| (root, r.joint, r.end))
        } else {
            solve_two_bone(tip, target, l2, l1, -side).map(|r| (r.end, r.joint, tip))
        }
        .ok_or_else(|| error("cannot solve these numeric IK parameters"))?;
        let (new_root, joint, end) = solve;
        let first = translation(new_root) * Mat4::rotate_z(angle(joint - new_root) - angle(elbow - root)) * translation(-root) * authored_upper;
        let second_world = translation(joint) * Mat4::rotate_z(angle(end - joint) - angle(tip - elbow)) * translation(-elbow) * authored_upper * authored_lower;
        let second = first.inverse().filter(|m| finite(*m)).ok_or_else(|| error("solved upper transform is not invertible"))? * second_world;
        if !finite(first) || !finite(second) {
            return Err(error("IK result exceeds the numeric range"));
        }
        // Extreme scale ratios/coordinates can make inverse compensation lose
        // precision despite finite coefficients. Never return a distorted pose.
        if !close_matrix(first * second, second_world) {
            return Err(error("IK transform compensation is numerically unstable; scale ratios or coordinates are too extreme"));
        }
        Ok(RigPose { upper: first, lower: second })
    }
}

pub(crate) fn local_matrix(ctx: &EvalCtx<'_>, layer: &Layer, par: f64) -> Result<Option<Mat4>, RigError> {
    let Some(rig) = ctx.comp.rigs2d.iter().find(|r| r.layers.contains(&layer.id)) else {
        return Ok(None);
    };
    let pose = ctx.rig2d_pose(rig)?;
    let matrix = if layer.id == rig.layers[0] { pose.upper } else { pose.lower };
    if par == 1.0 {
        return Ok(Some(matrix));
    }
    let tr = layer.transform().ok_or_else(|| RigError { rig: rig.id, message: "missing anchor".into() })?;
    let a = ctx.v3(layer, tr, "anchor", [0.0; 3]);
    let a = vec3(a[0], a[1], 0.0);
    Ok(Some(matrix * Mat4::translate(a) * Mat4::scale(vec3(par, 1.0, 1.0)) * Mat4::translate(-a)))
}

impl crate::Renderer<'_> {
    /// Validate the same IK inputs used by viewer, CPU/GPU placement and export,
    /// including active nested compositions at their mapped source times.
    pub fn validate_rigs(&self, comp: effectcraft_project::ItemId, time: effectcraft_time::Tick) -> Result<(), String> {
        if self.project.comps().all(|(_, c)| c.rigs2d.is_empty()) {
            return Ok(());
        }
        let mut pending = vec![(comp, time, 0usize)];
        let mut seen = std::collections::HashSet::new();
        let mut remaining = 4096usize;
        while let Some((id, time, depth)) = pending.pop() {
            if !seen.insert((id, time)) {
                continue;
            }
            remaining = remaining.checked_sub(1).ok_or("IK composition dependency work limit exceeded")?;
            if depth >= 64 {
                return Err("IK composition dependency depth limit exceeded".into());
            }
            let ctx = self.eval_ctx(id, time).ok_or("IK composition not found")?;
            for rig in &ctx.comp.rigs2d {
                ctx.rig2d_pose(rig).map_err(|e| format!("Composition {}: {e}", id.0))?;
            }
            for layer in &ctx.comp.layers {
                if layer.switches.video
                    && layer.is_active_at(time)
                    && let effectcraft_project::LayerSource::Comp { item } = layer.source
                {
                    if pending.len() >= 4096 {
                        return Err("IK composition dependency work limit exceeded".into());
                    }
                    pending.push((item, ctx.nested_time(layer), depth + 1));
                }
            }
        }
        Ok(())
    }
}
