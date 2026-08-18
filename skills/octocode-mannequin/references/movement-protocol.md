# Movement Protocol

Load when composing or validating a pose command — the wire format `scripts/skeleton.mjs pose` accepts.
Sign convention (X/Z/Y axes per `references/joint-constraints.md`): a movement name implies the signed axis value —
`flexion:90` ≡ `x:+90`, `extension:90` ≡ `x:-90`; `abduction`/`internalRotation`/`pronation` positive,
`adduction`/`externalRotation`/`supination` negative. Both members of an opposite pair (`flexion` + `extension`) on one joint in one command is a validation error — ambiguous intent.

## Command shape

```json
{ "pose": [ { "joint": "UpperArm_L", "movements": { "flexion": 90, "abduction": 20 } },
            { "joint": "Forearm_L",  "movements": { "flexion": 45, "pronation": 30 } },
            { "joint": "Head",       "movements": { "rotation": -30 } } ],
  "root": { "tx": 0, "ty": 1.1, "tz": -0.6, "pitch": -190, "yaw": 0, "roll": 0, "fly": true } }
```

The optional `root` is a whole-figure transform on the pelvis (the FK root): `tx/ty/tz` translate in metres,
`pitch/yaw/roll` orient in degrees (pitch = forward/back somersault, yaw = turn, roll = cartwheel), `fly` suspends
ground contact. Per-joint `movements` only fold the body around a fixed pelvis; **travel, turns, jumps, and a backflip (a −360° pitch through an airborne `ty` arc) require `root`**. It is keyframed kinematics, not simulated physics — there is no gravity, collision, or balance.

| Input | Result |
|---|---|
| `joint` not a name in `scripts/skeleton.mjs` `BONES` | distinct error |
| movement off that joint's `movements` allowlist (`references/joint-constraints.md`) — axis physically exists but is not anatomically valid for the joint, e.g. `abduction` on the knee | rejected, never silently applied |
| unknown movement name | distinct error |
| axis magnitude past the `references/rom-table.md` limit | clamped with a warning |

Retry with the reported valid movements / clamped value rather than guessing. The same rules apply whether the command arrives via the CLI, `window.applyPose`, or the WebMCP `apply_pose` tool (`references/webmcp-agent.md`).

## Resolution order

1. Normalize each `movements` entry to a signed per-axis value; reject opposite-pair collisions.
2. Swing-twist joints (shoulder, hip): combine X+Z into one swing vector, clamp its magnitude to the joint's cone
   limit (symmetric — a deliberate simplification of the real asymmetric ROM in `rom-table.md`), clamp Y twist
   independently, then recompose into a local quaternion.
3. Hinge/box joints: clamp each active axis independently against its own min/max, compose in X→Z→Y order.
4. Mirror the left side: for `_L` joints negate the lateral (z) and axial (y) geometry so abduction/rotation go the
   anatomically-opposite world direction from the right — ROM buckets and sagittal flexion are unaffected.
5. Walk the hierarchy from `Hips` applying each bone's local quaternion on top of its parent's world transform
   (forward kinematics) to get every bone's world position + orientation.

Multi-frame animations are an array of these commands (keyframes); validate one by running
`node scripts/skeleton.mjs sequence --file <frames.json>` before playing it.

Output: `{ bones: [{ name, parent, localQuat, worldPos, worldQuat, clamped, warnings }] }` — `clamped:true` marks a
joint whose requested angle was cut down to fit its ROM; `warnings` names which axis and by how much. Feed this straight
into `assets/viewer.template.html` (`references/viewer-guide.md`) or consume `worldPos`/`worldQuat` for any other renderer.

Next: generating/using the visual manikin → `references/viewer-guide.md`; provenance → `references/references.md`.
