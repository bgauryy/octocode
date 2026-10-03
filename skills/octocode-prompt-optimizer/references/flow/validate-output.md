# VALIDATE and OUTPUT

Load after FIX. Validate the complete draft before you write or present it.

## Shared checks

- [ ] Critical rules use proportionate enforcement; optional guidance stays optional.
- [ ] Every rule names an observable action and its boundary, and executes without rereading its section.
- [ ] A colleague with no context could follow the prompt.
- [ ] Each example pair is the smallest that separates the confused behaviors, and every example shows the wanted behavior.
- [ ] Every sentence directs an action, sets a boundary, defines a distinction, or explains a consequence.
- [ ] No conflicts, ambiguous referents, filler, or duplicate rule owners; intent, required branches, exact commands, and required frontmatter/metadata remain intact.
- [ ] Every branch has a trigger, action, output, and recovery; decisions route explicitly (IF/THEN or a table).
- [ ] Outputs have concrete shapes; examples and data are tagged where a reader can mistake them for instructions; every tag closes; critical rules sit where the agent finds them.
- [ ] The before/after score is on record; behavior justifies any material growth.

## Domain checks (only when the target uses the domain)

| Domain | Pass when | Owner |
|---|---|---|
| Tool / MCP surface | each rule in its owning layer; shared descriptors from one definition; every tool pair has a deciding condition | `../tools/tool-contracts.md` |
| MCP wire | negotiated version, paginated `tools/list`, schema-valid call/result, structured content, change signal, untrusted annotations | `../tools/mcp-wire-contract.md` |
| Cross-app contract | one semantic owner; native wire rules kept; invalid, denied, retry, cancel, stale, removal paths tested | `../agents/cross-app-contracts.md` |
| Runtime-assembled context | running surface, version, entrypoint, composition, visibility, lifetime, last observable input evidenced | `runtime-context.md` |
| Context budget | current limit and serialized occupancy measured; output reserved; cached tokens not counted as free | `../context/context-budget.md` |
| Economics / caching | current prices feed cost per success; cold/warm/tail-change check confirms cache telemetry; every miss classified | `../context/token-economics.md`, `../context/prompt-caching.md` |
| Frozen agent prompt | base version and digest match; task content is an append-only overlay | `../agents/agent-prompt-integrity.md` |

## Reliability claims

Wording judgment never proves reliability; use `octocode-eval-benchmark`. When it is unavailable: freeze baseline scenarios with verifiers and a held-out set, change one hypothesis, rerun the same cases, inspect raw calls, and keep the change only when the target metric improves without an unacceptable regression. Record failures as `Case | Stage | Symptom | Cause | Repair | Recheck`.

Final questions: Does it execute for every intended mode? Which sentence changes nothing the model does next (cut it)? Did any edit change intent (must be No)? Do not output after a failed check: repair local failures in FIX; return to UNDERSTAND when intent changed.

## OUTPUT

Write only when authorized; otherwise answer in chat. Report only successful writes.

| Request | Variant |
|---|---|
| Write or return a prompt, description, or rule text | The artifact only: no preamble, summary, or rationale |
| Rewrite, or format unspecified | Full optimized document + summary |
| Minimal edit, review-only, or unsafe/unavailable write | Patch-style delta |
| Multi-tool server | Contract audit table from `../tools/contract-audit.md` |

Document and delta variants report: `# Optimization Complete` with issues/fixes count, `intent preserved: Yes`, grade before → after, files changed; a `Category | Count | Example / reason` table; then `## Optimized Document` (full content) or `## Patch-Style Delta` (`Section | Before | After | Why`). If a requested change alters the repair, return to FIX and revalidate. The flow ends when you present the artifact and its truthful delta.
