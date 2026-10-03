# Packets

Load when you write worker briefs or parse returns.

| Request field (required) | Content |
|---|---|
| `goal` | one bounded objective |
| `context` | decisive facts and exact anchors only |
| `scope` | include, exclude, tools, stop rule |
| `authority` | allowed effects, approval gates, prohibitions |
| `budget` | worker and graph time, token, tool-call cap; replan threshold |
| `ownership` | **manager-as-tool** (parent keeps the requester) or **handoff** (specialist owns next turns, with a return or terminal rule); writes need disjoint paths and a verify command |
| `acceptance` | observable done criteria |
| `return` | required shape (structured prefixes or a schema) |

Optional technique fields: `playbook`, `playbook_owner`, `mimic`, `claim_table`, `questions` (`references/challenge.md`).

| Result field (required) | Content |
|---|---|
| `status` | `complete` \| `partial` \| `blocked` |
| `result` | the conclusion, no transcript |
| `evidence` | ≤8 decisive anchors (`path:line`, URL, command, artifact) |
| `verification` | check and outcome, or why not |
| `confidence` | confirmed \| likely \| uncertain, plus gaps |
| `next` | next action or `none` |

Re-ask only when a missing field blocks verification or the next action; accept an equivalent clear result.

- Message kinds: `request` · `question` · `status` · `result` · `blocker` · `approval-needed` · `cancel`.
- Strip transcripts, tool chatter, and unpaired tool history.
- Handoff: pass a short summary, not full worker history.

Next: run workers with `references/coordinate.md`; merge with `references/completion.md`.
