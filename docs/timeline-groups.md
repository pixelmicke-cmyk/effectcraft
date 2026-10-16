# Timeline groups

Groups are flat organizational composition data (`Comp::timeline_groups`), not
layers or transform parents. Create Group in the layer context menu groups the
selected layers. Group rows support collapse/expand, name editing (double click
or Rename Group), the standard label palette, adding/removing selected layers,
and Dissolve Group. These document edits, including collapse, are undoable and
saved with the project. Existing files default to no groups.

A layer belongs to at most one group. Adding it to another transfers membership.
Rows inside a group retain composition stack order; groups appear near their
first visible member. `Comp::layers` itself is never reordered by grouping.
Selecting a group name selects its actual layers; collapsing preserves selection.
Empty groups stay accessible. Deleting a layer prunes its membership in the same
Undo operation; missing references on load are repaired and reported.

Control tracks follow their owner layer. In particular, an IK track follows its
rig's root layer, even if its second bone is in another group. The rig still owns
its target Property, ID and keyframes. This keeps organizational data independent
of control implementations and extensible to future controls. Grouping, colors
and collapse are excluded from pixel-change comparisons. Rendering, parenting,
animation and viewer controls do not consult groups.

Command API: `timeline.group.create` (`layers`, optional `name`), `.add` and
`.remove` (`group`, `layers`), `.rename` (`group`, `name`), `.label` (`group`,
`label`), `.collapse` (`group`, `collapsed`), `.dissolve` (`group`). All accept
an optional `comp`. Omitting layers uses selected layers; omitting group on remove
removes those layers from any group. Group IDs share the project ID namespace
and are remapped with member layer IDs on composition duplication/import.

Nested groups, organizational drag-and-drop, and independent membership for
owner-bound controls are not implemented. Native layer reordering remains a
separate action that changes the composition stack as before.
