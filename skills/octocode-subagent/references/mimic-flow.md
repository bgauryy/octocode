# Mimic Flow
Load when a worker must follow another role’s instructions, skill excerpt, or prompt playbook without inheriting that agent’s chat. Why: context engineering beats dumping another agent’s full history (LangChain multi-agent: right data per agent).

## Pattern
**Instruction lending:** parent attaches a filtered playbook so the worker mimics a known good flow (review checklist, research algorithm, verify steps) while staying sealed.

## What you can share
| Share | Example |
|---|---|
| Skill excerpt / checklist | `octocode-research` claim ledger steps; roast severity ladder |
| Output contract | Required sections / return schema |
| Tool allowlist + stop rules | Same as packet `scope` |
| Short worked example | 1 tiny gold example of the needed return shape |

## What you must NOT share
| Never share | Why |
|---|---|
| Full parent or peer transcripts | Context poison + echo |
| Unverified worker prose as “facts” | Amplifies errors |
| Secrets, raw credentials, private user data | Safety |
| Entire foreign system prompts verbatim when rights/unknown | Prefer cite path + paraphrase duties |
| Competing mandatory rules from two playbooks | Conflicts → pick one owner |

## Packet fields (additions)
- `playbook` — path or pasted excerpt (≤1 screen) of steps to mimic
- `playbook_owner` — which skill/doc is authoritative if conflict
- `mimic` — `steps` | `tone` | `output_shape` (say which dimensions to copy)
- Still require normal `goal` / `acceptance` / `return`

## Rules
1. Mimic **procedure**, not conclusions — worker must still produce fresh evidence.
2. One playbook owner per packet.
3. If mimicking a critic/reviewer flow, give anchors + acceptance, not the author agent’s draft as gospel (pair with `references/interview.md` or verifier-critic).
4. Prefer linking skill refs the worker can load over pasting huge prompts.
5. After return, parent checks the worker followed the playbook’s **acceptance**, not that prose “sounds like” the other agent.

## When NOT
- Worker only needs one fact → put it in `context`, no playbook.
- Role is pure rubber-duck → `references/rubber-duck.md` (no foreign prompt needed).

Next: packets → `references/packets.md`; critique of returns → `references/interview.md`.
