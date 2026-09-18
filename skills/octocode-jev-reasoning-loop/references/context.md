# Context engineering

Load when estimating tokens, compacting a packet, or deciding what to include. Why: unnecessary context costs accuracy as well as tokens; missing evidence cannot be repaired by confident judgments; and the wrong context actively misleads Jev.

> *"Context engineering is the delicate art of filling the context window with just the right information for the next step."*
> — **Andrej Karpathy**

This is the operating principle for every Jev packet. Not the most information. Not the least. The *right* information for *this specific decision*. Every field that does not change Jev's answer wastes budget and dilutes signal. Every field that is missing but would change the answer invalidates the judgment.

## Schema selection as context engineering

Choosing the right schema is the first context engineering decision:

| Signal | Right schema | Wrong schema (too heavy) |
|---|---|---|
| Weak intuition, pre-verbal | `hunch.schema.json` | hypothesis-triage (overkill) |
| 2–5 competing explanations | `hypothesis-triage.schema.json` | claim-check (wrong contract) |
| Evidence collected, claim to check | `claim-check.schema.json` | hypothesis-triage (wrong phase) |
| About to assert, need grounding check | `hallucination-gate.schema.json` | any other schema |

Using a heavier schema than needed pads state with structure Jev does not need. Using a lighter one omits structure Jev requires. Match the schema to the signal.

## Working budget

Treat the “34k window” as a rough capacity assumption, not a safe request size: use the stricter verified limits in `references/protocol.md`. For planning, aim well below them; about 24k estimated tokens for state plus questions is a conservative operating target, not a provider limit or measured optimum. Most checks should be much smaller.

Budget the complete serialized state, instructions and criteria—not state alone. Leave room for formatting, estimation error and provider overhead. Use a model-compatible tokenizer if available; otherwise label estimates approximate and keep generous headroom. UTF-8 bytes, characters and the client's 4 MiB file cap are not token counts. Usage from a completed request is telemetry, not a preflight guarantee. No bundled tokenizer verifies fit; provider rejection means reduce the packet, not retry it unchanged.

## Compact without changing the question

Order the packet: decision and scope; decisive excerpts and strongest counterevidence; necessary definitions/constraints; unknowns and coverage. Deduplicate repeated extracts. Shorten background before evidence. Keep negations, guard branches, units, timestamps and provenance intact. Record omitted material as a coverage gap if it could affect the conclusion; do not claim an exhaustive review after dropping it.

Avoid sending the full conversation, unrelated source files, tool catalogs or this skill's instructions. Summarize background as background; keep deciding evidence exact and attributable. A summary of the host's belief is not a replacement for the observations that could falsify it.

## Partition by dependency

If the material still does not fit, partition by proposition or evidence dependency, not arbitrary byte chunks. Check independent premises separately; the host verifies and combines them. For a final cross-packet comparison, include all deciding anchors together. If those cannot fit, retain host reasoning or report unresolved scope rather than asking Jev to decide from lossy summaries.

Questions in one call are independent. A dependent question needs a later call with the verified premise and original evidence in state. Do not promote an earlier model answer into fact, omit its uncertainty, or accumulate a recursive history of opinions. Continue with `references/research.md` for the bounded dispute protocol.
