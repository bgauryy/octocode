# octocode-mannequin

Anatomically-named humanoid skeleton rig + range-of-motion-clamped movement protocol, drivable from a Node CLI
and rendered as an interactive Three.js manikin.

## What it does

Ships a 22-bone rig (pelvis→spine→chest→neck→head/jaw, per-side shoulder→upper arm→forearm→hand, per-side
upper leg→lower leg→foot→toes) in an arms-down anatomical-neutral rest pose, bilaterally mirrored, where every
joint carries a real DOF and clinical range-of-motion limit. A command like
`{"joint":"UpperArm_L","movements":{"flexion":90}}` is clamped to that joint's real ROM (swing-twist cone for
ball joints, box/hinge otherwise), run through forward kinematics, and rendered live in a self-contained Three.js
page — grounded so the figure stands on the floor, with smooth looped Catmull-Rom animation and per-joint sliders.
An optional root 6-DOF channel (translate + pitch/yaw/roll) moves, turns, and tumbles the whole figure, so the
same protocol expresses a walk, run, dance, or a backflip (keyframed, not physically simulated). Correctness is
enforced by a 335-check audit (no bone stretch, bilateral direction, ROM, allowlist, determinism, root transform).

## When to use

- Posing or animating a humanoid figure programmatically (agent-driven, not hand-keyframed)
- Letting an agent drive a live figure: the viewer exposes WebMCP tools (`get_scheme`, `apply_pose`,
  `play_sequence`, …) that a Claude/CDP agent discovers and invokes in the browser
- Explaining joint types/DOF/range-of-motion for a specific bone or movement
- Needing a Three.js skeleton to extend (add IK, physics, or a real mesh on top)

## Agent control (WebMCP)

The generated page is an **agent control surface**: it registers 11 page-native MCP tools via
`document.modelContext.registerTool` ([WebMCP](https://webmcp.dev/)). An agent launches Chrome with
`--enableFeatures WebMCP`, discovers the tools over the CDP `WebMCP` domain (via the `octocode-chrome-devtools`
skill), and controls the figure both in **joint space** (`apply_pose`, absolute or `relative` jog) and **task
space** (`reach` a hand/foot to a world point, `look_at` a point) plus `play_sequence`, `set_ground`, `set_camera`
— the same state the human sliders drive. `get_scheme` returns a self-describing `guide` (world frame, units, sign
conventions, examples) so an unfamiliar agent drives it correctly first try; every tool enforces ROM clamps and
returns `warnings`/`errors`/`reached` for closed-loop control. A green "WebMCP: N tools live" badge confirms
registration. See `references/webmcp-agent.md`.

## When not to use

Physics/ragdoll simulation, IK foot-locking, balance, collision, motion-capture (BVH/FBX) import, or a
sculpted/skinned character mesh — this stays a keyframed capsule-and-sphere rig (a backflip is authored, not
simulated); see `references/references.md` for why.

## How it works

`SCHEME → COMMAND → CLAMP+FK → RENDER → DRIVE`

```bash
node scripts/skeleton.mjs scheme                                   # inspect bones/joints/ROM/movements
node scripts/skeleton.mjs pose --cmd '{"pose":[{"joint":"UpperArm_L","movements":{"flexion":90}}]}'
node scripts/skeleton.mjs sequence --file assets/run-cycle.json    # validate an animation (keyframes)
node scripts/skeleton.mjs viewer --out mannequin.html --pose pose.json
open mannequin.html   # drag sliders, or window.playSequence(<frames>,{loop:true}) in the console
```

No `npm install` — `scripts/skeleton.mjs` has zero dependencies; the viewer loads Three.js from a CDN via an
import map, so it needs network access to open.

## Install

Copy or symlink this folder into your agent skills root (e.g. `~/.claude/skills/` or project `.agents/skills/`).

```bash
npx octocode skill --add --path . --platform claude,cursor,agents --mode symlink
```
