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
2. Movement commands use anatomical names valid for that joint (`references/movement-protocol.md`), never raw
   signed degrees or a movement outside the joint's allowlist — the CLI and the tools reject those.
3. Treat ROM values as rig defaults, not medical fact (`references/rom-table.md`).

## Workflow

1. `node scripts/skeleton.mjs scheme` — inspect bones + joint DOF/ROM/movements (`references/anatomy-scheme.md`,
   `references/joint-constraints.md`, `references/rom-table.md`).
2. Compose a pose command (`references/movement-protocol.md`), run `node scripts/skeleton.mjs pose --cmd '...'`,
   read `warnings`/`clamped`; validate an animation with `skeleton.mjs sequence --file frames.json`.
3. `node scripts/skeleton.mjs viewer --out <file>.html [--pose pose.json]`, open it, drag sliders or call
   `window.applyPose`/`window.playSequence` from the console; ready animations in `assets/`:
   `run-cycle.json`, `backflip.json`, `dance.json`, `idle.json` (alive breathing/sway) (`references/viewer-guide.md`).
4. Agent-drive it live: serve the viewer's WebMCP tools over CDP (`references/webmcp-agent.md`) — 11 tools incl.
   task-space `reach` (put a hand/foot at a world point) and `look_at`; `get_scheme.guide` is the frame/units
   contract. Discover, invoke, verify with `get_pose` or a screenshot.

## When NOT this skill

General 3D scene setup, or physics/ragdoll simulation, IK foot-locking, balance, collision, or mocap (BVH/glTF)
import. Motion is keyframed kinematics — a backflip is authored, not simulated (`references/references.md`).

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

- `scripts/skeleton.mjs` — `scheme` / `pose` / `sequence` / `viewer` subcommands (`--help`); zero npm installs. WebMCP CDP mechanics are delegated to the `octocode-chrome-devtools` skill.
- `scripts/eval-mannequin.mjs` — `--self-test` / `--triggers` / `--scheme` / `--case <id> --input <answer.md>` after skill edits.
