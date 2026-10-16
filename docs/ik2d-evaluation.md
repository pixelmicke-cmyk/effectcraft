# Two-segment IK with nonuniform scale

EffectCraft evaluates column-vector layer matrices as
`L = T(position) R(rotation) S(scale) T(-anchor)` and a child as
`Wchild = Wparent Lchild`. An anisotropic `S` does not commute with `R`.
Adding an Euler angle to the lower layer therefore cannot generally produce a
rigid rotation of its existing artwork.

The rig evaluator instead returns complete local matrices. Let `A` and `B` be
the authored upper and lower local matrices, and `P` the external ancestor
product. In the root parent's coordinate system the authored matrices are
`A` and `AB`. The shoulder, elbow and explicit tip are transformed into this
same coordinate system. Their saved distances are the solver's segment lengths;
neither local pixel lengths nor averaged Scale X/Y values are used.

The analytic two-circle solver finds the new joints, clamping the target to
`abs(length1 - length2) .. length1 + length2`. For each segment construct a rigid
correction `Ci = T(new joint) R(angle delta) T(-old joint)`. The returned matrices
are:

```text
A' = C1 A
B' = inverse(A') C2 A B
P A' B' = P C2 A B
```

Thus every point of the lower artwork receives the same rigid correction in the
solve space. Its edge lengths, angles and proportions are retained, including any
shear already present through authored parenting. `B'` may contain compensation
shear: this is intentional and must not be decomposed back into authored TRS
properties. The upper artwork is likewise transformed rigidly. With a similarity
transform `P`, these guarantees also hold in composition coordinates, with the
parent's common scale factor. Parent translation, rotation and uniform scale
continue to apply normally. The untouched bind target returns the exact authored
matrices, preserving the initial pose. Target animation never edits layer data.

The same derivation applies when the root is controlled and the tip is fixed.
Both bend directions use the existing geometric solver. Recomposition is checked
to reject finite but numerically unstable inverse compensation.

## Deliberate boundaries

Positive, static nonuniform scale is supported on both rig layers. Nonuniform
scale on external ancestors remains unsupported: it changes the Euclidean metric
and world-space reach into an ellipse. It must not silently use circular reach
limits in that case. Reflections, singular transforms, non-square source pixels,
3D, auto-orient, transform expressions, animated scale/anchors/bone positions,
overlapping rigs and rigged ancestors retain explicit errors.

Connect still uses Parent & Link. When automatic parenting would require shear
that the authored TRS model cannot store, Connect rejects the operation atomically
instead of changing the initial artwork. Already parented anisotropic chains use
the full matrix compensation above without changing Scale, rotation or keyframes.

## Timeline animation

Each rig has a UI-only control row before its root layer. Expanding it shows the
existing target Property, with X/Y values, a stopwatch and keyframes. This row is
never put in `Comp.layers`, is not a render input, and does not offer layer switches,
effects or Parent & Link. The latter continues to operate on the actual bones.

The common `prop.get`, `prop.set`, `prop.toggleAnimation`, `prop.addKey`,
`prop.toggleKey`, `prop.select` and `prop.deselect` commands accept an explicit
`rig` ID and the target `prop` UID (or `path: "position"`). `keys.select` accepts
the same address inside its `keys` list. Existing key move/delete/interpolation/
easing commands then operate on the selection. Key references retain their legacy
`layer` field, which now denotes an animation-owner ID in the shared global ID
namespace. A rig ID in that field never denotes or creates a layer.

Rig key times are always composition time, unaffected by bone in/out, start time
or time stretch. The same Property interpolation is read by the viewer, renderer,
nested compositions and export. Viewer dragging follows the normal stopwatch
rule: a static Property changes its value, an animated Property inserts/updates
a key at the CTI. There is currently no separate editor Auto-Key switch.

Control expansion is UI state; selections are kept per composition and sanitized
on project/history changes. Key edits use the existing project Undo/Redo history,
including one history step per drag. Merely selecting a control is not a document
edit, consistent with layer selection. Copy Value / Paste Value uses the shared
Property value clipboard, copying both Position components together. Pasting onto
a key preserves its time, easing and interpolation; the Position context menu
pastes at the CTI and creates a key if needed. The value Graph Editor, animation
presets and full-keyframe clipboard workflows for controls remain unsupported.

Each rig owns a standard project `Label`, independently of its bones. The control
track's context menu uses the same label picker and preference palette as layers;
the viewer handle and track swatch use that label's color. `rig.setLabel2d` edits
only the label, with normal document Undo/Redo. Old rig files default to Blue.
Labels are UI metadata and do not change the rendered image or target animation.
