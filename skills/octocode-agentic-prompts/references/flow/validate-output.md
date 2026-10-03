# VALIDATE and OUTPUT

Load after FIX. Validate the complete draft before you write or present it.

## Shared checks (beyond `SKILL.md` § VALIDATE)

- [ ] Critical rules use proportionate enforcement; optional guidance stays optional.
- [ ] Each example pair is the smallest that separates the confused behaviors.
- [ ] Every sentence passes the RATE Density test.
- [ ] Decisions route explicitly.
- [ ] Outputs have concrete shapes; examples and data are tagged where they could pass for instructions; critical rules sit where the agent finds them.
- [ ] Behavior justifies any material growth.

## Domain checks (only when the target uses the domain)

| Domain | Pass when | Owner |
|---|---|---|
| Tool / MCP surface | each rule in its owning layer; every tool pair has a deciding condition; wire matches the negotiated version | `../tools/tool-contracts.md` |
| Cross-app contract | one semantic owner; change gate paths tested | `../agents/cross-app-contracts.md` |
| Runtime-assembled context | running surface and last observable input evidenced | `runtime-context.md` |
| Context budget | occupancy measured; output reserved | `../context/context-budget.md` |
| Economics / caching | cost per success from current prices; cache check run | `../context/token-economics.md` |
| Frozen agent prompt | base version and digest match; task content is an append-only overlay | `../agents/agent-communication.md` |

## Reliability fallback

When `octocode-eval-benchmark` is unavailable: freeze baseline scenarios with verifiers and a held-out set, change one hypothesis, rerun the same cases, and keep the change only when the target metric improves without an unacceptable regression. Record `Case | Stage | Symptom | Cause | Repair | Recheck`.

## OUTPUT

| Request | Variant |
|---|---|
| Write or return a prompt, description, or rule text | No preamble, summary, or rationale |
| Format unspecified | Full document + summary |
| Unsafe or unavailable write | Patch-style delta |
| Multi-tool server | Contract audit table from `../tools/tool-contracts.md` |

Document and delta variants report issues fixed, grade before → after (when RATE ran), and files changed; add a `Category | Count | Example / reason` table only when it helps the reader. A requested change that alters the repair returns to FIX. The flow ends when you present the artifact and its truthful delta.
