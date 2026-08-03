# Viewer Guide

Load when generating or driving the visual manikin — after a pose exists (`references/movement-protocol.md`).

`node scripts/skeleton.mjs viewer --out mannequin.html [--pose pose.json]` writes a self-contained HTML file:
Three.js loads from the `unpkg` CDN via an ESM import map (no npm install, no build step), so it only runs
where the browser can reach the CDN. Open it directly (`file://...`) or serve it — no server required.

## What's inside

- Capsule (limb) + sphere (joint) primitives built from `BONES` lengths — no skinned mesh/rig file needed,
  matching the "agent-driven, not artist-skinned" recommendation from the Three.js research pass.
- `OrbitControls` for camera (drag to rotate, scroll to zoom, right-drag to pan).
- One slider per active axis per joint, labeled by real movement names (`flex / ext`, `abd / add`, …), grouped
  by body region, hard-bounded to `references/rom-table.md`'s min/max — exceeding a limit is impossible by construction.
- **Grounding** (on by default): the whole rig shifts each frame so the lowest foot sits on the floor, so the
  figure stands rather than floats and its pelvis bobs with a gait. `window.setGround(false)` disables it.
- The same clamp + `sideSign` bilateral math from `scripts/skeleton.mjs`, ported to vanilla JS (duplicated
  intentionally: one algorithm, two runtimes; the bone/joint/ROM *data* has one source, serialized in at
  generation time).

## Driving it programmatically

Regenerate with a new `--pose pose.json` to bake in a starting pose, or open the page and call, from the browser
console / `octocode-chrome-devtools`: `window.applyPose(poseCommand)` for an instant snap, or
`window.playSequence([poseCommand, ...], {frameMs, loop})` to animate. Interpolation is a **looped Catmull-Rom**
over the resolved joint angles (C1-continuous, ~46× lower jerk than linear and no seam pop — measured), re-clamped
to ROM every frame; a joint a frame omits holds its previous value, so a run cycle is a handful of keyframes.
Ship-ready example: `assets/run-cycle.json` (validate any sequence first with
`node scripts/skeleton.mjs sequence --file <frames.json>`). `window.stopSequence()` halts a loop; `window.mannequin`
exposes `{scene, camera, controls, boneMap, jointState}` for anything else.

**Whole-figure motion**: a keyframe's optional `root` (`references/movement-protocol.md`) translates/turns/tumbles
the pelvis, so `playSequence` can travel, turn, jump, and backflip — `assets/backflip.json` is a ready example
(play it **one-shot**, `loop:false`; it ends on a crouch-absorb→stand landing settle). Pass `{launch:{apex:1.3}}`
(or `{v0}`) to give airborne (`fly`) frames a real gravity parabola (`ty = v₀t − ½gt²`).

Root interpolation is **clamped to each interval's bracketing keyframes** so Catmull-Rom cannot overshoot (no
rotating past the upright landing, no foot below the floor — the two bugs that made flip ends look wrong), and a
`loop` **accumulates net root delta** each cycle, so a looped spin keeps rotating forward instead of un-flipping
at the seam. Still deterministic kinematics, not physics — no ground reaction, balance, or collision.

For an agent to drive this page as a live MCP server (not console/sliders), the viewer also registers WebMCP
tools — see `references/webmcp-agent.md`.

Next: provenance for every claim above → `references/references.md`.
