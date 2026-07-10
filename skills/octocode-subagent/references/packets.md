# Packets

Load when writing worker briefs or parsing returns. Why: workers see no parent chat.

## Request (required)
- `goal` — one bounded objective
- `context` — decisive facts + exact anchors only
- `scope` — include / exclude / tools / stop
- `ownership` — **manager-as-tool** (parent keeps user) vs **handoff** (specialist owns next turns + return/terminal rule). Writes need disjoint paths + verify cmd
- `acceptance` — observable done criteria
- `return` — required shape (structured prefixes or schema OK)

## Result (required)
- `status` — `complete` | `partial` | `blocked`
- `result` — conclusion; no transcript
- `evidence` — ≤8 decisive anchors (`path:line`, URL, cmd, artifact)
- `verification` — check + outcome, or why not
- `confidence` — confirmed | likely | uncertain + gaps
- `next` — next action or `none`

Empty or missing `return` shape = failed handback — re-ask or replan.

## Message kinds
`request` · `question` · `status` · `result` · `blocker` · `approval-needed` · `cancel`

Map remote A2A `input-required` / `auth-required` to parent/user gates — do not auto-continue.

## Token / handoff filter
Pass goal, anchors, scope, acceptance, return shape. Strip transcripts, tool chatter, and unpaired tool history. Prefer a short summary over full worker history on handoff.

Next: `coordinate.md` · `synthesize.md` · `a2a.md`.
