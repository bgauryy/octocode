---
name: octocode-mannequin
description: "Use when posing, animating, or explaining a 3D human skeleton/manikin: anatomical joints, ROM clamps, walk/run/dance/backflip sequences, Three.js viewer, or agent-driven WebMCP figure control (reach/look_at). Phrases like pose the manikin, skeleton scheme, animate a walk cycle. Not for general 3D scenes, physics/ragdoll, IK foot-locking, or mocap import."
---

# Octocode Mannequin

Anatomically-named 22-bone humanoid rig (arms-down neutral, bilaterally mirrored) plus a movement protocol that
clamps every joint to real range of motion, with a root 6-DOF channel (translate/pitch/yaw/roll) for travel,
turns, jumps and backflips — a Node CLI, a self-contained Three.js viewer (grounded, smooth Catmull-Rom
animation), and WebMCP tools an agent drives live. Flow: `SCHEME → COMMAND → CLAMP+FK → RENDER → DRIVE`.

## Hard rules

1. `scripts/skeleton.mjs` is the single source of truth for bones/joints/ROM/movements; references and the
   viewer mirror it. The viewer re-implements the clamp/`sideSign`/FK math — any change must be made in BOTH
   files and re-verified (they must stay identical).
2. Movement commands use anatomical names valid for that joint, never raw signed degrees and never a movement
   outside the joint's allowlist — read `references/movement-protocol.md` before composing one; the CLI and the
   tools reject those.
3. Treat ROM values as rig defaults, not medical fact — read `references/rom-table.md` before quoting a limit.

## Gate

Writing a viewer file (`viewer --out <file>.html`) overwrites that path, and agent-driving needs a fresh
non-headless Chrome on a new port: confirm the output path and get the user's OK before the first write or
browser launch.

## Workflow

1. Run `node scripts/skeleton.mjs scheme` first to inspect bones + joint DOF/ROM/movements; for the hierarchy read
   `references/anatomy-scheme.md`, for the constraint model read `references/joint-constraints.md`, and for exact
   degrees read `references/rom-table.md`.
2. Compose a pose command using `references/movement-protocol.md`, run `node scripts/skeleton.mjs pose --cmd '...'`,
   read `warnings`/`clamped`; then validate an animation by running `skeleton.mjs sequence --file frames.json`.
3. To render, run `node scripts/skeleton.mjs viewer --out <file>.html [--pose pose.json]`, open it, drag sliders or
   call `window.applyPose`/`window.playSequence` from the console — ready animations live in `assets/`
   (`run-cycle.json`, `backflip.json`, `dance.json`, `idle.json` for alive breathing/sway); for the viewer's
   mechanics read `references/viewer-guide.md`.
4. To agent-drive it live, load `references/webmcp-agent.md` and serve the viewer's WebMCP tools over CDP — 11 tools
   incl. task-space `reach` (put a hand/foot at a world point) and `look_at`; `get_scheme.guide` is the frame/units
   contract. Discover, invoke, verify with `get_pose` or a screenshot.

Stop when: the pose verifies via `get_pose` or a screenshot; `clamped:true` shows the joint is already at its ROM
limit (that is the answer — do not push past it); a movement is rejected as invalid for the joint (retry with the
reported allowlist, never with raw degrees); `reach` returns `reached:false` with a residual distance (the target is
out of range — re-target, do not re-invoke unchanged); tool discovery returns `WEBMCP_NO_TOOLS` or fewer than 11
tools (fall back to `window.applyPose` via `Runtime.evaluate`); screenshots come back blank (Chrome is headless —
relaunch non-headless); or the request needs physics, IK foot-locking, or mocap (out of scope, say so).

## When NOT this skill

General 3D scene setup, or physics/ragdoll simulation, IK foot-locking, balance, collision, or mocap (BVH/glTF)
import. Motion is keyframed kinematics — a backflip is authored, not simulated; for why that scope was
chosen read `references/references.md`.

## Progressive refs

| Ref | When |
|---|---|
| `references/anatomy-scheme.md` | Building/explaining the bone hierarchy |
| `references/joint-constraints.md` | DOF/constraint model + verified rotation directions per joint |
| `references/rom-table.md` | Exact degree limits per movement |
| `references/movement-protocol.md` | Writing/validating a pose command |
| `references/viewer-guide.md` | Generating/driving the Three.js viewer |
| `references/webmcp-agent.md` | Agent-driving the manikin via WebMCP + CDP |
| `references/references.md` | Provenance / why a design choice was made |

## Scripts

- Run `node scripts/skeleton.mjs` for the `scheme` / `pose` / `sequence` / `viewer` subcommands (`--help`); zero npm installs. WebMCP CDP mechanics are delegated to the `octocode-chrome-devtools` skill.
