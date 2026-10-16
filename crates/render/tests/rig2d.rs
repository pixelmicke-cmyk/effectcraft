use effectcraft_color::Label;
use effectcraft_geom::{Vec3, vec3};
use effectcraft_project::{
    Comp, ItemId, ItemKind, LayerId, LayerSource, Project, Solid, Value, build,
    rig2d::{BendDirection, DrivenEnd, Rig2d},
};
use effectcraft_render::{EvalCtx, LayerCache, NoFootage, RenderOpts, Renderer};
use effectcraft_time::{FrameRate, Tick};

fn fixture() -> (Project, ItemId, LayerId, LayerId, LayerId) {
    let mut p = Project::default();
    let mut c = Comp::new(256, 128, FrameRate::FPS_24, Tick::from_seconds_f64(2.0));
    let mut body = build::layer(&mut p, &c, "Body", LayerSource::Null, (1, 1), None);
    body.props.prop_mut("transform/anchor").unwrap().value = Value::Vec3([0.0; 3]);
    body.props.prop_mut("transform/position").unwrap().value = Value::Vec3([0.0; 3]);
    let body_id = body.id;
    c.layers.push(body);
    let mut ids = vec![];
    for (name, width, position) in [("Upper", 50, [40.0, 64.0, 0.0]), ("Lower", 40, [50.0, 4.0, 0.0])] {
        let item = p.add_item(name, Label::Red, None, ItemKind::Solid(Solid { color: [1.0, 0.0, 0.0], width, height: 8, pixel_aspect: 1.0 }));
        let mut layer = build::layer(&mut p, &c, name, LayerSource::Solid { item }, (width, 8), None);
        layer.props.prop_mut("transform/anchor").unwrap().value = Value::Vec3([0.0, 4.0, 0.0]);
        layer.props.prop_mut("transform/position").unwrap().value = Value::Vec3(position);
        layer.parent = Some(ids.first().copied().unwrap_or(body_id));
        ids.push(layer.id);
        c.layers.push(layer);
    }
    let mut rig = Rig2d::new(&mut p, [ids[0], ids[1]], [[40.0, 64.0], [90.0, 64.0], [130.0, 64.0]], [40.0, 4.0], [130.0, 64.0]).unwrap();
    rig.bend_direction = BendDirection::Positive;
    c.rigs2d.push(rig);
    let cid = p.add_item("Arm", Label::Sandstone, None, ItemKind::Comp(c.into()));
    (p, cid, body_id, ids[0], ids[1])
}

fn target(p: &mut Project, cid: ItemId, value: [f64; 2]) {
    p.comp_mut(cid).unwrap().rigs2d[0].target.value = Value::Vec2(value);
}
fn points(p: &Project, cid: ItemId, time: Tick) -> [Vec3; 3] {
    let c = p.comp(cid).unwrap();
    let ctx = EvalCtx::new(p, cid, c, time);
    let rig = &c.rigs2d[0];
    ctx.rig2d_pose(rig).unwrap();
    let a = c.layer(rig.layers[0]).unwrap();
    let b = c.layer(rig.layers[1]).unwrap();
    [ctx.world_matrix(a).apply(vec3(0.0, 4.0, 0.0)), ctx.world_matrix(b).apply(vec3(0.0, 4.0, 0.0)), ctx.world_matrix(b).apply(vec3(40.0, 4.0, 0.0))]
}
fn near(a: Vec3, b: Vec3) {
    assert!((a.x - b.x).hypot(a.y - b.y) < 1e-8, "{a:?} != {b:?}");
}
fn lengths(p: [Vec3; 3], scale: f64) {
    assert!(((p[1].x - p[0].x).hypot(p[1].y - p[0].y) - 50.0 * scale).abs() < 1e-8);
    assert!(((p[2].x - p[1].x).hypot(p[2].y - p[1].y) - 40.0 * scale).abs() < 1e-8);
}

#[test]
fn target_controls_pose_both_bends_and_reach_limits() {
    for side in [BendDirection::Positive, BendDirection::Negative] {
        let (mut p, cid, _, _, _) = fixture();
        p.comp_mut(cid).unwrap().rigs2d[0].bend_direction = side;
        for (value, expected) in [([100.0, 40.0], [100.0, 40.0]), ([1000.0, 64.0], [130.0, 64.0]), ([40.0, 64.0], [50.0, 64.0])] {
            target(&mut p, cid, value);
            let solved = points(&p, cid, Tick::ZERO);
            lengths(solved, 1.0);
            near(solved[2], vec3(expected[0], expected[1], 0.0));
        }
        target(&mut p, cid, [110.0, 64.0]);
        let solved = points(&p, cid, Tick::ZERO);
        assert_eq!(solved[1].y > 64.0, side == BendDirection::Positive);
    }
}

#[test]
fn untouched_target_preserves_authored_pose_and_initial_elbow_side() {
    let (mut p, cid, _, a, b) = fixture();
    p.comp_mut(cid).unwrap().rigs2d.clear();
    for (id, angle) in [(a, -30.0), (b, 60.0)] {
        p.comp_mut(cid).unwrap().layer_mut(id).unwrap().props.prop_mut("transform/rotation").unwrap().value = Value::Scalar(angle);
    }
    let c = p.comp(cid).unwrap();
    let ctx = EvalCtx::new(&p, cid, c, Tick::ZERO);
    let matrices = [ctx.world_matrix(c.layer(a).unwrap()), ctx.world_matrix(c.layer(b).unwrap())];
    let joints = [matrices[0].apply(vec3(0.0, 4.0, 0.0)), matrices[1].apply(vec3(0.0, 4.0, 0.0)), matrices[1].apply(vec3(40.0, 4.0, 0.0))];
    let rig = Rig2d::new(&mut p, [a, b], joints.map(|p| [p.x, p.y]), [40.0, 4.0], [joints[2].x, joints[2].y]).unwrap();
    p.comp_mut(cid).unwrap().rigs2d.push(rig);
    let c = p.comp(cid).unwrap();
    let ctx = EvalCtx::new(&p, cid, c, Tick::ZERO);
    assert_eq!(ctx.world_matrix(c.layer(a).unwrap()), matrices[0]);
    assert_eq!(ctx.world_matrix(c.layer(b).unwrap()), matrices[1]);
}

#[test]
fn body_parent_rotation_translation_and_scale_are_respected() {
    let (mut p, cid, body, _, _) = fixture();
    for (path, value) in [
        ("transform/position", Value::Vec3([20.0, 10.0, 0.0])),
        ("transform/rotation", Value::Scalar(30.0)),
        ("transform/scale", Value::Vec3([150.0, 150.0, 100.0])),
    ] {
        p.comp_mut(cid).unwrap().layer_mut(body).unwrap().props.prop_mut(path).unwrap().value = value;
    }
    let c = p.comp(cid).unwrap();
    let ctx = EvalCtx::new(&p, cid, c, Tick::ZERO);
    let parent = ctx.world_matrix(c.layer(body).unwrap());
    let wanted = parent.apply(vec3(100.0, 40.0, 0.0));
    target(&mut p, cid, [wanted.x, wanted.y]);
    let solved = points(&p, cid, Tick::ZERO);
    near(solved[0], parent.apply(vec3(40.0, 64.0, 0.0)));
    near(solved[2], wanted);
    lengths(solved, 1.5);
}

#[test]
fn root_driven_rig_keeps_the_tip_fixed() {
    let (mut p, cid, _, _, _) = fixture();
    {
        let rig = &mut p.comp_mut(cid).unwrap().rigs2d[0];
        rig.driven_end = DrivenEnd::Root;
        rig.bind_target = Some([40.0, 64.0]);
    }
    target(&mut p, cid, [70.0, 40.0]);
    let solved = points(&p, cid, Tick::ZERO);
    near(solved[0], vec3(70.0, 40.0, 0.0));
    near(solved[2], vec3(130.0, 64.0, 0.0));
    lengths(solved, 1.0);
}

#[test]
fn animation_is_in_composition_time_and_never_writes_bone_properties() {
    let (mut p, cid, _, a, b) = fixture();
    for id in [a, b] {
        let l = p.comp_mut(cid).unwrap().layer_mut(id).unwrap();
        l.start_time = Tick::from_seconds_f64(-0.5);
        let r = l.props.prop_mut("transform/rotation").unwrap();
        r.set_animated(true, Tick::ZERO);
        r.set_value_at(Tick::from_seconds_f64(1.0), Value::Scalar(20.0));
    }
    let rig = &mut p.comp_mut(cid).unwrap().rigs2d[0];
    rig.target.value = Value::Vec2([95.0, 40.0]);
    rig.target.set_animated(true, Tick::ZERO);
    rig.target.set_value_at(Tick::from_seconds_f64(1.0), Value::Vec2([105.0, 40.0]));
    let before = p.clone();
    for (seconds, x) in [(0.0, 95.0), (0.5, 100.0), (1.0, 105.0)] {
        let solved = points(&p, cid, Tick::from_seconds_f64(seconds));
        near(solved[2], vec3(x, 40.0, 0.0));
        lengths(solved, 1.0);
    }
    assert_eq!(p, before);
    let loaded = Project::from_json(&p.to_file_json().unwrap()).unwrap();
    assert_eq!(p, loaded);
    assert_eq!(points(&p, cid, Tick::ZERO), points(&loaded, cid, Tick::ZERO));
}

#[test]
fn viewer_placement_cpu_render_and_cache_use_identical_transforms() {
    let (mut p, cid, _, a, b) = fixture();
    target(&mut p, cid, [100.0, 40.0]);
    let cache = LayerCache::default();
    let mut r = Renderer::new(&p, &NoFootage, RenderOpts::default());
    r.cache = Some(&cache);
    let c = p.comp(cid).unwrap();
    let ctx = EvalCtx::new(&p, cid, c, Tick::ZERO);
    for id in [a, b] {
        let l = c.layer(id).unwrap();
        let buf = r.layer_buf(&ctx, l).unwrap();
        assert_eq!(ctx.layer_to_comp(l).0, r.placement(&ctx, l, &buf).matrices[0]);
    }
    let image = r.comp_frame(cid, Tick::ZERO);
    let again = r.comp_frame(cid, Tick::ZERO);
    assert_eq!(image.data, again.data);
    let mut baked = p.clone();
    let pose = ctx.rig2d_pose(&c.rigs2d[0]).unwrap();
    // Bake only into a test oracle: the normal render remains entirely read-only.
    for (id, m) in [(a, pose.upper), (b, pose.lower)] {
        let tr = baked.comp_mut(cid).unwrap().layer_mut(id).unwrap().transform_mut().unwrap();
        tr.get_mut("rotation").unwrap().value = Value::Scalar(m.0[1][0].atan2(m.0[0][0]).to_degrees());
    }
    baked.comp_mut(cid).unwrap().rigs2d.clear();
    let expected = Renderer::new(&baked, &NoFootage, RenderOpts::default()).comp_frame(cid, Tick::ZERO);
    assert_eq!(image.data, expected.data);
    target(&mut p, cid, [110.0, 80.0]);
    let mut cached = Renderer::new(&p, &NoFootage, RenderOpts::default());
    cached.cache = Some(&cache);
    assert_eq!(cached.comp_frame(cid, Tick::ZERO).data, Renderer::new(&p, &NoFootage, RenderOpts::default()).comp_frame(cid, Tick::ZERO).data);
}

#[test]
fn nested_compositions_map_time_and_render_the_same_arm() {
    let (mut p, cid, _, _, _) = fixture();
    target(&mut p, cid, [100.0, 40.0]);
    let target = &mut p.comp_mut(cid).unwrap().rigs2d[0].target;
    target.set_animated(true, Tick::ZERO);
    target.set_value_at(Tick::from_seconds_f64(1.0), Value::Vec2([110.0, 80.0]));
    let mut outer = Comp::new(256, 128, FrameRate::FPS_24, Tick::from_seconds_f64(2.0));
    let mut l = build::layer(&mut p, &outer, "Nested", LayerSource::Comp { item: cid }, (256, 128), None);
    l.start_time = Tick::from_seconds_f64(0.5);
    outer.layers.push(l);
    let outer_id = p.add_item("Outer", Label::Sandstone, None, ItemKind::Comp(outer.into()));
    let r = Renderer::new(&p, &NoFootage, RenderOpts::default());
    r.validate_rigs(outer_id, Tick::from_seconds_f64(1.0)).unwrap();
    assert_eq!(r.comp_frame(cid, Tick::from_seconds_f64(0.5)).data, r.comp_frame(outer_id, Tick::from_seconds_f64(1.0)).data);
}

#[test]
fn projects_saved_before_bind_target_was_added_still_evaluate() {
    let (mut p, cid, body, _, _) = fixture();
    p.comp_mut(cid).unwrap().rigs2d[0].bind_target = None;
    p.comp_mut(cid).unwrap().layer_mut(body).unwrap().props.prop_mut("transform/position").unwrap().value = Value::Vec3([20.0, 0.0, 0.0]);
    let text = p.to_file_json().unwrap();
    assert!(!text.contains("bind_target"));
    let (mut loaded, notes) = Project::from_json_repaired(&text).unwrap();
    assert!(notes.iter().any(|n| n.contains("original target")));
    assert_eq!(loaded.comp(cid).unwrap().rigs2d[0].bind_target, Some([130.0, 64.0]));
    near(points(&loaded, cid, Tick::ZERO)[2], vec3(150.0, 64.0, 0.0));
    target(&mut loaded, cid, [100.0, 40.0]);
    near(points(&loaded, cid, Tick::ZERO)[2], vec3(100.0, 40.0, 0.0));
}

#[test]
fn an_animated_target_remains_active_when_it_passes_the_bind_target() {
    let (mut p, cid, body, _, _) = fixture();
    p.comp_mut(cid).unwrap().layer_mut(body).unwrap().props.prop_mut("transform/position").unwrap().value = Value::Vec3([20.0, 0.0, 0.0]);
    let target = &mut p.comp_mut(cid).unwrap().rigs2d[0].target;
    target.set_animated(true, Tick::ZERO);
    target.set_value_at(Tick::from_seconds_f64(1.0), Value::Vec2([110.0, 40.0]));
    let solved = points(&p, cid, Tick::ZERO);
    near(solved[2], vec3(130.0, 64.0, 0.0));
    lengths(solved, 1.0);
}

#[test]
fn unsupported_inputs_and_dependency_cycles_report_errors_without_recursion() {
    for case in ["scale", "expression", "cycle", "overlap", "missing", "changed", "target"] {
        let (mut p, cid, body, a, _) = fixture();
        target(&mut p, cid, [100.0, 40.0]);
        match case {
            "scale" => p.comp_mut(cid).unwrap().layer_mut(body).unwrap().props.prop_mut("transform/scale").unwrap().value = Value::Vec3([200.0, 100.0, 100.0]),
            "expression" => p.comp_mut(cid).unwrap().rigs2d[0].target.expr = Some(effectcraft_project::Expression { text: "value".into(), enabled: true }),
            "cycle" => p.comp_mut(cid).unwrap().layer_mut(body).unwrap().parent = Some(a),
            "overlap" => {
                let r = p.comp(cid).unwrap().rigs2d[0].clone();
                p.comp_mut(cid).unwrap().rigs2d.push(r);
            }
            "missing" => p.comp_mut(cid).unwrap().layer_mut(body).unwrap().parent = Some(LayerId(999_999)),
            "changed" => p.comp_mut(cid).unwrap().rigs2d[0].rest_joints[1][0] = 95.0,
            "target" => p.comp_mut(cid).unwrap().rigs2d[0].target.value = Value::Vec2([f64::NAN, 0.0]),
            _ => unreachable!(),
        }
        let c = p.comp(cid).unwrap();
        let ctx = EvalCtx::new(&p, cid, c, Tick::ZERO);
        assert!(ctx.rig2d_pose(&c.rigs2d[0]).is_err());
        assert!(Renderer::new(&p, &NoFootage, RenderOpts::default()).validate_rigs(cid, Tick::ZERO).is_err());
        let m = ctx.world_matrix(c.layer(a).unwrap());
        assert!(m.0.iter().flatten().all(|v| v.is_finite()));
    }
}

fn scaled_fixture(scales: [[f64; 3]; 2]) -> (Project, ItemId, LayerId, LayerId, LayerId) {
    let (mut p, cid, body, a, b) = fixture();
    for (id, scale, rotation) in [(body, [125.0, 125.0, 100.0], 17.0), (a, scales[0], 23.0), (b, scales[1], -47.0)] {
        let tr = p.comp_mut(cid).unwrap().layer_mut(id).unwrap().transform_mut().unwrap();
        tr.get_mut("scale").unwrap().value = Value::Vec3(scale);
        tr.get_mut("rotation").unwrap().value = Value::Scalar(rotation);
    }
    // Capture bind geometry exactly as Connect does: world anchors/end point,
    // transformed back through the external parent's inverse.
    let authored = points(&p, cid, Tick::ZERO);
    let c = p.comp(cid).unwrap();
    let ctx = EvalCtx::new(&p, cid, c, Tick::ZERO);
    let inv = ctx.world_matrix(c.layer(body).unwrap()).inverse().unwrap();
    let rig = &mut p.comp_mut(cid).unwrap().rigs2d[0];
    rig.rest_joints = authored.map(|v| {
        let v = inv.apply(v);
        [v.x, v.y]
    });
    let bind = [authored[2].x, authored[2].y];
    rig.bind_target = Some(bind);
    rig.target.value = Value::Vec2(bind);
    (p, cid, body, a, b)
}

fn assert_artwork_metric(a: effectcraft_geom::Mat4, b: effectcraft_geom::Mat4) {
    // Preserving three independent edge lengths also preserves the angle
    // between them, detecting both unwanted stretching and lost inherited shear.
    for v in [vec3(1.0, 0.0, 0.0), vec3(0.0, 1.0, 0.0), vec3(1.0, 1.0, 0.0)] {
        let av = a.apply(v) - a.apply(Vec3::ZERO);
        let bv = b.apply(v) - b.apply(Vec3::ZERO);
        assert!((av.x.hypot(av.y) - bv.x.hypot(bv.y)).abs() < 1e-8);
    }
}

#[test]
fn scaled_rotated_parented_bones_preserve_artwork_and_reach_both_driven_ends() {
    for scales in [[[100.0, 100.0, 100.0], [63.3, 61.5, 100.0]], [[150.0, 75.0, 100.0], [80.0, 120.0, 100.0]]] {
        for driven in [DrivenEnd::Tip, DrivenEnd::Root] {
            for bend in [BendDirection::Positive, BendDirection::Negative] {
                let (mut p, cid, body, a, b) = scaled_fixture(scales);
                let initial = points(&p, cid, Tick::ZERO);
                let c = p.comp(cid).unwrap();
                let ctx = EvalCtx::new(&p, cid, c, Tick::ZERO);
                let original = [ctx.world_matrix(c.layer(a).unwrap()), ctx.world_matrix(c.layer(b).unwrap())];
                let parent = ctx.world_matrix(c.layer(body).unwrap());
                let rest = c.rigs2d[0].rest_joints;
                let l1 = (rest[1][0] - rest[0][0]).hypot(rest[1][1] - rest[0][1]);
                let l2 = (rest[2][0] - rest[1][0]).hypot(rest[2][1] - rest[1][1]);
                let fixed = if driven == DrivenEnd::Tip { 0 } else { 2 };
                let controlled = 2 - fixed;
                let pivot = rest[fixed];
                let rig = &mut p.comp_mut(cid).unwrap().rigs2d[0];
                rig.driven_end = driven;
                rig.bend_direction = bend;
                let bind = [initial[controlled].x, initial[controlled].y];
                rig.bind_target = Some(bind);
                rig.target.value = Value::Vec2(bind);
                let layers = p.comp(cid).unwrap().layers.clone();
                for distance in [l1.max(l2), l1 + l2 + 100.0, 0.0] {
                    let goal = parent.apply(vec3(pivot[0] + 0.8 * distance, pivot[1] + 0.6 * distance, 0.0));
                    target(&mut p, cid, [goal.x, goal.y]);
                    let solved = points(&p, cid, Tick::ZERO);
                    near(solved[fixed], initial[fixed]);
                    for (v, length) in [(solved[1] - solved[0], l1), (solved[2] - solved[1], l2)] {
                        assert!((v.x.hypot(v.y) - length * 1.25).abs() < 1e-8);
                    }
                    if distance == l1.max(l2) {
                        near(solved[controlled], goal);
                    }
                    let radius = (solved[controlled].x - solved[fixed].x).hypot(solved[controlled].y - solved[fixed].y);
                    assert!(
                        (radius - distance.clamp((l1 - l2).abs(), l1 + l2) * 1.25).abs() < 1e-8,
                        "{scales:?} {driven:?} {bend:?}: distance={distance}, radius={radius}"
                    );
                    let c = p.comp(cid).unwrap();
                    let ctx = EvalCtx::new(&p, cid, c, Tick::ZERO);
                    for (id, original) in [a, b].into_iter().zip(original) {
                        assert_artwork_metric(original, ctx.world_matrix(c.layer(id).unwrap()));
                    }
                    assert_eq!(c.layers, layers);
                }
            }
        }
    }
}

#[test]
fn scaled_viewer_render_cache_and_reopened_project_use_full_affine_matrices() {
    let (mut p, cid, _, a, b) = scaled_fixture([[150.0, 75.0, 100.0], [63.3, 61.5, 100.0]]);
    let cache = LayerCache::default();
    for goal in [[80.0, 70.0], [110.0, 90.0]] {
        target(&mut p, cid, goal);
        let before = p.clone();
        let c = p.comp(cid).unwrap();
        let ctx = EvalCtx::new(&p, cid, c, Tick::ZERO);
        let mut renderer = Renderer::new(&p, &NoFootage, RenderOpts::default());
        renderer.cache = Some(&cache);
        renderer.validate_rigs(cid, Tick::ZERO).unwrap();
        for id in [a, b] {
            let l = c.layer(id).unwrap();
            let buf = renderer.layer_buf(&ctx, l).unwrap();
            assert_eq!(ctx.layer_to_comp(l).0, renderer.placement(&ctx, l, &buf).matrices[0]);
        }
        let image = renderer.comp_frame(cid, Tick::ZERO);
        assert!(image.data.iter().flatten().any(|v| *v > 0.0));
        assert_eq!(image.data, Renderer::new(&p, &NoFootage, RenderOpts::default()).comp_frame(cid, Tick::ZERO).data);
        let reopened = Project::from_json(&p.to_file_json().unwrap()).unwrap();
        assert_eq!(points(&p, cid, Tick::ZERO), points(&reopened, cid, Tick::ZERO));
        assert_eq!(image.data, Renderer::new(&reopened, &NoFootage, RenderOpts::default()).comp_frame(cid, Tick::ZERO).data);
        assert_eq!(p, before);
    }
}
