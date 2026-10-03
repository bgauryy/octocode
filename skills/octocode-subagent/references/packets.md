# Packets

Load when you write worker briefs or parse returns. Check what context the host lets the worker inherit; provide only what it lacks.

## Request (required)

- `goal`: one bounded objective.
- `context`: decisive facts and exact anchors only.
- `scope`: include, exclude, tools, stop rule.
- `authority`: allowed effects, approval gates, explicit prohibitions. Workers cannot widen parent authority.
- `budget`: worker and graph time, token, and tool-call cap, plus a replan threshold.
- `ownership`: **manager-as-tool** (parent keeps the requester) or **handoff** (specialist owns next turns, with a return or terminal rule). Writes need disjoint paths and a verify command.
- `acceptance`: observable done criteria.
- `return`: required shape (structured prefixes or a schema).
- Optional technique fields: `playbook`, `playbook_owner`, `mimic`, `claim_table`, `questions` (`references/challenge.md`).

## Result (required)

- `status`: `complete` | `partial` | `blocked`.
- `result`: the conclusion, no transcript.
- `evidence`: ≤8 decisive anchors (`path:line`, URL, command, artifact).
- `verification`: check and outcome, or why not.
- `confidence`: confirmed | likely | uncertain, plus gaps.
- `next`: next action or `none`.

Re-ask only when a missing field blocks verification or the next action. Accept an equivalent clear result; do not repeat work only to enforce a template.

## Messages and filter

- Message kinds: `request` · `question` · `status` · `result` · `blocker` · `approval-needed` · `cancel`.
- Map remote A2A `input-required` and `auth-required` to parent or user gates. Do not auto-continue.
- Pass goal, anchors, scope, acceptance, and return shape. Strip transcripts, tool chatter, and unpaired tool history.
- On handoff, pass a short summary, not full worker history.
- Give verifier workers the artifact, anchors, and acceptance contract, not the executor's reasoning transcript.

Next: run workers with `references/coordinate.md`; merge with `references/completion.md`.
