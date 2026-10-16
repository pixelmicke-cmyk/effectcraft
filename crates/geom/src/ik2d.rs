use crate::{Vec2, vec2};

/// Resultatet av en IK-beräkning för två sammanlänkade delar.
#[derive(Clone, Copy, Debug)]
pub struct Ik2dResult {
    pub joint: Vec2,
    pub end: Vec2,
    /// Första segmentets absoluta vinkel i grader.
    pub first_angle: f64,
    /// Andra segmentets absoluta vinkel i grader, inte relativ till det första.
    pub second_angle: f64,
}

/// Beräknar en tvådelad IK-kedja och begränsar målet till kedjans räckvidd.
/// Negativ bend_direction böjer åt ena sidan; noll och positiva värden åt den andra.
/// Sammanfallande rot och mål använder positiv x som referensriktning.
/// Returnerar None för ogiltiga indata eller resultat som inte kan representeras.
pub fn solve_two_bone(root: Vec2, target: Vec2, first_length: f64, second_length: f64, bend_direction: f64) -> Option<Ik2dResult> {
    if !first_length.is_finite()
        || !second_length.is_finite()
        || first_length <= 0.0
        || second_length <= 0.0
        || !root.x.is_finite()
        || !root.y.is_finite()
        || !target.x.is_finite()
        || !target.y.is_finite()
        || !bend_direction.is_finite()
    {
        return None;
    }

    // Subtract directly when possible, preserving small differences at large origins.
    // Opposite extreme coordinates may overflow: half-sized differences cannot.
    let delta = target - root;
    let (delta, factor) = if delta.x.is_finite() && delta.y.is_finite() { (delta, 1.0) } else { (target * 0.5 - root * 0.5, 2.0) };
    let delta_scale = delta.x.abs().max(delta.y.abs());
    let (direction, distance) = if delta_scale > 0.0 {
        let unit = delta / delta_scale;
        let norm = unit.x.hypot(unit.y);
        (unit / norm, (delta_scale / first_length.max(second_length)) * norm * factor)
    } else {
        (vec2(1.0, 0.0), 0.0)
    };

    // Work in units of the longest bone: sums and squares stay bounded even
    // when physical lengths or the distance between coordinates exceed f64::MAX.
    let scale = first_length.max(second_length);
    let a = first_length / scale;
    let b = second_length / scale;
    if a == 0.0 || b == 0.0 {
        return None;
    }
    let reach = distance.clamp((a - b).abs(), a + b);
    let along = if a == b {
        reach * 0.5
    } else if reach == 0.0 {
        0.0
    } else {
        0.5 * (reach + (a - b) * ((a + b) / reach))
    };
    let along = along.clamp(-a, a);
    let height = ((a - along) * (a + along)).max(0.0).sqrt();
    let side = if bend_direction < 0.0 { -1.0 } else { 1.0 };
    let perpendicular = vec2(-direction.y, direction.x);
    let first = direction * along + perpendicular * (height * side);
    let second = direction * reach - first;
    let joint = vec2(first.x.mul_add(scale, root.x), first.y.mul_add(scale, root.y));
    let end_offset = direction * reach;
    let end = vec2(end_offset.x.mul_add(scale, root.x), end_offset.y.mul_add(scale, root.y));
    if !joint.x.is_finite() || !joint.y.is_finite() || !end.x.is_finite() || !end.y.is_finite() {
        return None;
    }

    Some(Ik2dResult { joint, end, first_angle: first.y.atan2(first.x).to_degrees(), second_angle: second.y.atan2(second.x).to_degrees() })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(actual: f64, expected: f64, scale: f64) {
        assert!((actual / scale - expected / scale).abs() < 1e-12, "{actual} != {expected}");
    }

    fn lengths(root: Vec2, result: Ik2dResult, a: f64, b: f64) {
        let first = result.joint - root;
        let second = result.end - result.joint;
        close(first.x.hypot(first.y), a, a.max(b));
        close(second.x.hypot(second.y), b, a.max(b));
        assert!(result.first_angle.is_finite() && result.second_angle.is_finite());
    }

    #[test]
    fn reachable_targets_preserve_lengths_at_different_scales() {
        for scale in [1e-200, 1.0, 1e200, 1e307] {
            for side in [-1.0, 0.0, 1.0] {
                let root = vec2(-2.0 * scale, scale);
                let target = root + vec2(1.2 * scale, 0.8 * scale);
                let result = solve_two_bone(root, target, scale, scale, side).unwrap();
                lengths(root, result, scale, scale);
                close(result.end.x, target.x, scale);
                close(result.end.y, target.y, scale);
            }
        }
    }

    #[test]
    fn coincident_root_and_target_preserve_both_bones() {
        for (a, b) in [(100.0, 100.0), (100.0, 60.0), (60.0, 100.0)] {
            for side in [-1.0, 1.0] {
                let root = vec2(10.0, -20.0);
                let result = solve_two_bone(root, root, a, b, side).unwrap();
                lengths(root, result, a, b);
                close((result.end - root).x, (a - b).abs(), a.max(b));
                close(result.end.y, root.y, a.max(b));
            }
        }
    }

    #[test]
    fn unreachable_targets_clamp_to_inner_and_outer_limits() {
        for (a, b) in [(100.0, 60.0), (60.0, 100.0), (100.0, 100.0)] {
            for distance in [1.0_f64, 40.0, 160.0, 1000.0] {
                let root = vec2(0.0, 0.0);
                let target = vec2(0.0, -distance);
                let result = solve_two_bone(root, target, a, b, 1.0).unwrap();
                lengths(root, result, a, b);
                close(result.end.x, 0.0, a.max(b));
                close(-result.end.y, distance.clamp((a - b).abs(), a + b), a.max(b));
            }
        }
    }

    #[test]
    fn extreme_coordinates_and_lengths_do_not_overflow() {
        let root = vec2(-1e308, 0.0);
        let result = solve_two_bone(root, vec2(1e308, 0.0), 1e308, 1e308, 1.0).unwrap();
        lengths(root, result, 1e308, 1e308);
        close(result.end.x, 1e308, 1e308);
        let result = solve_two_bone(vec2(0.0, 0.0), vec2(f64::MAX, f64::MAX), 3.0, 4.0, 1.0).unwrap();
        lengths(vec2(0.0, 0.0), result, 3.0, 4.0);
        close(result.end.x.hypot(result.end.y), 7.0, 7.0);
        assert!(solve_two_bone(vec2(f64::MAX, 0.0), vec2(f64::MAX, 0.0), 1.0, f64::MAX, 1.0).is_none());
    }

    #[test]
    fn bend_direction_mirrors_joint_and_angles_are_absolute() {
        let root = vec2(0.0, 0.0);
        let positive = solve_two_bone(root, vec2(100.0, 0.0), 100.0, 100.0, 1.0).unwrap();
        let negative = solve_two_bone(root, vec2(100.0, 0.0), 100.0, 100.0, -1.0).unwrap();
        close(positive.joint.x, negative.joint.x, 100.0);
        close(positive.joint.y, -negative.joint.y, 100.0);
        close(positive.first_angle, 60.0, 180.0);
        close(positive.second_angle, -60.0, 180.0);
    }

    #[test]
    fn invalid_parameters_are_rejected() {
        let zero = vec2(0.0, 0.0);
        for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(solve_two_bone(vec2(invalid, 0.0), zero, 1.0, 1.0, 1.0).is_none());
            assert!(solve_two_bone(vec2(0.0, invalid), zero, 1.0, 1.0, 1.0).is_none());
            assert!(solve_two_bone(zero, vec2(invalid, 0.0), 1.0, 1.0, 1.0).is_none());
            assert!(solve_two_bone(zero, vec2(0.0, invalid), 1.0, 1.0, 1.0).is_none());
            assert!(solve_two_bone(zero, zero, 1.0, 1.0, invalid).is_none());
        }
        for invalid in [0.0, -1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(solve_two_bone(zero, zero, invalid, 1.0, 1.0).is_none());
            assert!(solve_two_bone(zero, zero, 1.0, invalid, 1.0).is_none());
        }
    }
}
