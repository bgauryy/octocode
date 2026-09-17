# Context budgeting

Load when estimating tokens, compacting a large packet or splitting reasoning across calls. Why: unnecessary context costs accuracy as well as tokens, and missing evidence cannot be repaired by confident judgments.

## Working budget

Treat the “34k window” as a rough capacity assumption, not a safe request size: use the stricter verified limits in `references/protocol.md`. For planning, aim well below them; about 24k estimated tokens for state plus questions is a conservative operating target, not a provider limit or measured optimum. Most checks should be much smaller.

Budget the complete serialized state, instructions and criteria—not state alone. Leave room for formatting, estimation error and provider overhead. Use a model-compatible tokenizer if available; otherwise label estimates approximate and keep generous headroom. UTF-8 bytes, characters and the client's 4 MiB file cap are not token counts. Usage from a completed request is telemetry, not a preflight guarantee. No bundled tokenizer verifies fit; provider rejection means reduce the packet, not retry it unchanged.

## Compact without changing the question

Order the packet: decision and scope; decisive excerpts and strongest counterevidence; necessary definitions/constraints; unknowns and coverage. Deduplicate repeated extracts. Shorten background before evidence. Keep negations, guard branches, units, timestamps and provenance intact. Record omitted material as a coverage gap if it could affect the conclusion; do not claim an exhaustive review after dropping it.

Avoid sending the full conversation, unrelated source files, tool catalogs or this skill's instructions. Summarize background as background; keep deciding evidence exact and attributable. A summary of the host's belief is not a replacement for the observations that could falsify it.

## Partition by dependency

If the material still does not fit, partition by proposition or evidence dependency, not arbitrary byte chunks. Check independent premises separately; the host verifies and combines them. For a final cross-packet comparison, include all deciding anchors together. If those cannot fit, retain host reasoning or report unresolved scope rather than asking Jev to decide from lossy summaries.

Questions in one call are independent. A dependent question needs a later call with the verified premise and original evidence in state. Do not promote an earlier model answer into fact, omit its uncertainty, or accumulate a recursive history of opinions. Continue with `references/research.md` for the bounded dispute protocol.
