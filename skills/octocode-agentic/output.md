# Output

<!-- report-contract {"statuses":["Proposed","Ready","Needs work","Keep","Discard","Accept","Revert","Inconclusive","Invalid","Resolved","Waiting","Blocked"],"required":[],"anyOf":[["Findings","Changes","Decision","Outcome"]]} -->

Answer in chat unless a [saved report](#saved-report) is needed. Lead with the result; keep raw traces in linked evidence and include operational identifiers only when the receiver needs them. Put assumptions and uncertainty beside affected claims.

## Saved result

Approved edits to the project's own code, prompts or skills stay at their source paths. When saving other artifacts, use `<workspace>/.octocode/octocode-agentic/`. `<workspace>` is the repository root, else the current directory, and is never inside an existing `.octocode/`. Scratch files go in `<workspace>/.octocode/tmp/octocode-agentic/`. A path the user or a parent names wins. Reuse existing durable evidence rather than copying it into another record.

| Result | Path |
|---|---|
| Design, audit, prompt review, standalone prompt, coordination status, interview report | `<YYYY-MM-DD>-<slug>.md` |
| [Cold interview](references/interview.md) rounds | `<workspace>/.octocode/tmp/octocode-agentic/<run>/round-<n>/`, linked from the report |
| [Conversation audit](references/conversation-audit.md) | `<YYYY-MM-DD>-<slug>/` with `prompting-practices.md` and the private `user-messages.md` |
| Measured result evidence | Link native runner output; create `<YYYY-MM-DD>-<slug>/` only when evidence needs a new home. See [run folders](references/eval-harness.md#suite-and-run-folders). |

Never save inside the installed skill. A small chat edit needs no file.

## Saved report

Save when requested, required by project instructions or needed to preserve durable evidence across steps. Retain the evidence supporting measured results; existing durable runner output suffices. Coordination messages and evidence already live in the database; a chat summary naming `activity`, `locks` or `view` usually suffices. Save a separate coordination report when its open conflicts or multi-agent outcome need a durable handoff.

Use the [shared header](references/report-output.md#report-header), a `#` title and a lead stating the answer. Accepted `status` values: `Proposed`, `Ready`, `Needs work`, `Keep`, `Discard`, `Accept`, `Revert`, `Inconclusive`, `Invalid`, `Resolved`, `Waiting`, `Blocked`. Choose status and sections by report type:

| Report | `status` | Sections |
|---|---|---|
| Design or audit | `Proposed` (design not yet built), `Ready` (audit passed or design verified), `Needs work` | [`## Findings`](references/report-sections.md#findings), then `## Checks` |
| Prompt or instruction | `Ready`, `Needs work` | `## Findings` for a review, or `## Changes` (one row per edit) for a change report. Put a requested rewrite in full in a fenced block under `## Replacement` |
| Measurement | `Keep`, `Discard` (development evidence); `Accept`, `Revert`, `Inconclusive`, `Invalid` ([final verdict](references/eval-kpi.md#verdict)) | `## Question`, `## Conditions`, [`## Results`](references/report-sections.md#results), `## Guardrails`, `## Limits`, `## Decision` |
| Coordination | `Resolved`, `Waiting`, `Blocked` | `## Outcome`, [`## Pending`](references/report-sections.md#pending) |

Write `None.` under a required section that does not apply.

## Presentation

Use the [shared format guide](references/report-output.md#choose-the-format), and load its Mermaid or widget reference only for that syntax. After delivery, follow the [HTML offer](references/presentation.md#html-delivery). Use the [report commands](references/report-output.md#render-and-retrieve) for accepted rendering and for `--manifest` discovery.
