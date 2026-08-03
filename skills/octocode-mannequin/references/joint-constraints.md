# Joint Constraints

Load when clamping or explaining a joint's degrees of freedom — after `anatomy-scheme.md`, before `rom-table.md`.

Local bone axes (rig convention, not a universal anatomy standard): **+Y** = long axis toward the child (twist
axis) · **X** = flexion(+)/extension(−) · **Z** = abduction(+)/adduction(−). Ball-and-socket joints clamp X+Z as
one swing cone plus an independent Y twist (swing-twist decomposition); everything else clamps each active axis
as an independent box limit. Swing-twist is the correct model for shoulder/hip because it bounds cone + twist
separately, matching physics-engine `ConeTwistConstraint` (ammo.js/cannon-es); box clamps are what
`CCDIKSolver`/`three-ik` use and are adequate for the smaller-range joints below. [Daniel Holden, Joint Limits](https://theorangeduck.com/page/joint-limits) · [cannon-es ConeTwistConstraint](https://pmndrs.github.io/cannon-es/docs/classes/ConeTwistConstraint.html) · [three.js CCDIKSolver](https://threejs.org/docs/#examples/en/animations/CCDIKSolver)

| Joint (parent→bone) | Real joint | Type | DOF | Constraint model |
|---|---|---|---|---|
| Hips→Spine | Lumbar+thoracic spine | Facet/gliding, combined | 3 (X,Y,Z) | box |
| Neck→Head | Cervical spine | Condyloid+pivot, combined | 3 (X,Y,Z) | box |
| Head→Jaw | Temporomandibular (TMJ) | Ginglymoarthrodial (hinge+glide) | 2 (X,Z) | hinge+box |
| Chest→Shoulder | Sternoclavicular+AC+scapulothoracic, combined | Saddle+plane, non-synovial mix | 3 (X,Y,Z) | box |
| Shoulder→UpperArm | Glenohumeral (shoulder) | Ball-and-socket | 3 (X,Z swing + Y twist) | swing-twist |
| UpperArm→Forearm | Elbow + radioulnar, combined | Hinge + pivot | 2 (X + Y twist) | hinge+twist |
| Forearm→Hand | Wrist (radiocarpal+midcarpal) | Condyloid | 2 (X,Z) | box |
| Hips→UpperLeg | Hip | Ball-and-socket | 3 (X,Z swing + Y twist) | swing-twist |
| UpperLeg→LowerLeg | Knee | Modified hinge | 1 (X) | hinge |
| LowerLeg→Foot | Ankle (talocrural+subtalar) | Hinge + plane, combined | 2 (X,Z) | hinge+box |
| Foot→Toes | MTP (toes, combined) | Condyloid, simplified | 1 (X) | hinge |

**Dropped by design** (flagged in research, not modeled): knee axial rotation when flexed, TMJ's linear
(mm) translation component, per-finger/per-toe joints. Chest and Neck carry no joint row — they are 0-DOF
spacers (`references/anatomy-scheme.md`).

**Verified, not assumed**: the uniform X/Z/Y convention doesn't automatically produce the anatomically-correct
world-space direction for every bone's rest orientation (e.g. hip and knee flexion both "rotate +X on a
down-pointing bone" yet must swing opposite ways). Every joint × movement was checked by displacing an off-axis
marker and comparing against the expected direction (290-check audit), not by inspection — as a result
`Jaw`/`LowerLeg`/`Foot` carry a `flip` sign correction and `Shoulder`/`Hand` carry an `axisMap` override.

**Bilateral symmetry**: left and right limbs are mirror images — left abduction/rotation must go the opposite
world direction from right. `scripts/skeleton.mjs` applies a `sideSign` (−1 for `_L` bones) to the lateral (z)
and axial (y) geometry only, leaving sagittal flexion (x) and the ROM buckets untouched, so left abduction still
clamps to the abduction limit while swinging left. Verified for both sides in the audit.

Executable form: `JOINTS[].dof`, `.axes`, `.constraint` in `scripts/skeleton.mjs` — this table must stay in sync
with that array; treat the script as authoritative on disagreement.

Next: degree limits per axis → `references/rom-table.md`; command format that drives these clamps →
`references/movement-protocol.md`.
