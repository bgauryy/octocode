# WebMCP Agent Control

Load when an agent should drive the running manikin as a live MCP server instead of editing pose files. The
viewer registers page-native tools via `document.modelContext.registerTool` (the WebMCP standard —
[webmcp.dev](https://webmcp.dev/), [webmachinelearning/webmcp](https://github.com/webmachinelearning/webmcp));
an agent discovers and invokes them over Chrome's experimental `WebMCP` CDP domain.

## Tools the page exposes (11)

An agent has both **joint-space** control (set/jog angles) and **task-space** control (reach a world point) — the
minimal robotics/animation primitive set (pose + trajectory + reach + look-at + root) that composes into any
movement. Coordinate frame is right-handed, **+X right, +Y up, +Z forward**, angles in **degrees**, translation
in **metres**; `get_scheme.guide` returns this contract so an unfamiliar agent drives it correctly first try.

| Tool | Kind | Input | Purpose |
|---|---|---|---|
| `get_scheme` | read | — | **Call first.** The agent contract (`guide`: frame, units, conventions, examples) + bones + joints/movements. |
| `describe_joint` | read | `{joint}` | One joint's constraint + each movement's max degrees. |
| `get_pose` | read | — | Nonzero joint values + world positions of head/hands/feet (verify without a screenshot). |
| `apply_pose` | mutating | `{pose, root?, merge?, relative?}` | Set joint angles; `root` moves/turns/tumbles the whole figure; `relative:true` jogs from current (incremental). Returns clamp `warnings`/`errors` — read them to close the loop. |
| `reach` | mutating | `{effector, target:[x,y,z]}` | Task-space IK: put a hand/foot at a world point; solves the angles. Returns `reached` + residual `distance` (unreachable ⇒ reached:false). |
| `look_at` | mutating | `{joint:'Head', target:[x,y,z]}` | Aim the head to face a world point (gaze). Returns `alignment`. |
| `play_sequence` | mutating | `{frames, frameMs?, loop?, launch?}` | Smooth Catmull-Rom animation; frames may carry `root` (travel/turn/backflip); `launch:{apex}` = gravity arc. |
| `stop_sequence` / `reset_pose` | mutating | — | Halt a loop / return to arms-down rest. |
| `set_camera` | mutating | `{view}` or `{azimuth,elevation,distance}` | Frame a screenshot. |
| `set_ground` | mutating | `{enabled}` | Toggle floor contact (on by default). |

Read tools don't mutate, but the current Chrome build labels every tool `risk=mutating` (registerTool
`annotations` don't reach CDP yet) — rely on this table. The page also embeds the same contract as a
machine-readable `<script id="mcp-agent-guide">` block. The list call reports 11 tools.

## Driving loop (mechanics owned by `octocode-chrome-devtools`)

1. Generate the viewer (`references/viewer-guide.md`), then launch a **fresh, non-headless** Chrome with WebMCP
   on and the file loaded — headless has no WebGL so nothing renders (tools still work, but screenshots are blank):
   `open-browser.mjs --port <p> --enableFeatures WebMCP --url file://<abs>/mannequin.html`.
2. Discover: `WEBMCP_ACTION=list … webmcp-tools.mjs` — expect the 11 tools above, not `WEBMCP_NO_TOOLS`.
3. Invoke: `WEBMCP_ACTION=invoke WEBMCP_TOOL=apply_pose WEBMCP_INPUT='<json>'`. Read `[WEBMCP_RESULT]`; the tool's
   own `errors`/`warnings` tell you if a movement was invalid for the joint or clamped to its range of motion.
4. Verify by `get_pose` or a `set_camera` + screenshot — do not assume; confirm the figure moved.

Reused CDP sessions can't add the WebMCP flag — always a fresh port. `window.applyPose`/`playSequence` remain as
a `Runtime.evaluate` fallback if the WebMCP domain is unavailable.

Next: tool payloads mirror the wire format in `references/movement-protocol.md`; joint/ROM limits in
`references/rom-table.md`.
