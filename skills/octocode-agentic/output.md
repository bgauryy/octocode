# Output

<!-- report-contract {"statuses":["Proposed","Ready","Needs work","Keep","Discard","Accept","Revert","Inconclusive","Invalid","Resolved","Waiting","Blocked"],"required":[],"anyOf":[["Findings","Changes","Decision","Outcome"]]} -->

Answer in chat unless a [saved report](#saved-report) is needed. Lead with the result; keep raw traces in linked evidence and include operational identifiers only when the receiver needs them. Put assumptions and uncertainty beside affected claims.

## Response

BLUF: lead with the decision, verdict or outcome and its confidence. Then give what it rests on and the next check.

| Work | After the lead |
|---|---|
| Design or audit | The layer it affects (context, tools, loop, routing, communication, trust), evidence, trade-offs, limits |
| Prompt or instruction | The prompt in the requested format. For an edit: a full replacement, a change summary and verification. For a review: the affected text, its behavioral effect and a repair. Ready-to-paste task prompts include the deciding checks from [question selection](references/question-selection.md) |
| Measurement | The question and checks, then for a comparison: baseline and candidate conditions (scope, budget), metrics, guardrails, uncertainty, coverage, failures, cost |
| Coordination | Any pending conflict and who must act. Message and lease identifiers stay in the protocol unless another agent must act on them ([Pending](#pending)) |

## Design answer

1. **Recommendation.** State the architecture or contract in one sentence, with the workload assumption it rests on.
2. **Why.** Name the deciding trade-offs: correctness, cost, latency, context size, operational risk. Label each number measured, documented or estimated.
3. **Alternatives.** Name the simpler option considered (one loop, one model, one tool) and when a richer one wins.
4. **Failure modes.** Say how it could loop, flood context, drift after compaction, repeat a side effect or cross a trust boundary, and how each is detected and stopped.
5. **Check.** Give the task set, trace, contract test or metric that confirms it, with its pass condition.

Include only the parts that change the decision, and only failure modes reachable for this design.

For a contract (tool response, handoff, message envelope), show the typed shape, one valid example and one rejected example.

## Audit findings

Order findings worst first. Give each its location (file, tool, prompt section or trace step), symptom, mechanism, consequence, fix, and the check that proves the fix. Keep proven defects, observed in a trace or run, apart from candidates inferred from configuration or source alone. List the checklist items you could not assess, and why.

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
| Design or audit | `Proposed` (design not yet built), `Ready` (audit passed or design verified), `Needs work` | `## Findings`, then `## Checks` |
| Prompt or instruction | `Ready`, `Needs work` | `## Findings` for a review, or `## Changes` (one row per edit) for a change report. Put a requested rewrite in full in a fenced block under `## Replacement` |
| Measurement | `Keep`, `Discard` (development evidence); `Accept`, `Revert`, `Inconclusive`, `Invalid` ([final verdict](references/evaluation.md#verdict)) | `## Question`, `## Conditions`, `## Results`, `## Guardrails`, `## Limits`, `## Decision` |
| Coordination | `Resolved`, `Waiting`, `Blocked` | `## Outcome`, `## Pending` |

Write `None.` under a required section that does not apply.

### Findings

Use one table, most severe first:

| Location | Finding | Layer | Severity | Consequence | Confidence | Fix |
|---|---|---|---|---|---|---|

Severity is `Critical`, `High`, `Medium` or `Low`. Confidence is `high`, `medium` or `low`. A candidate gets `low`, and its Finding starts with `Candidate:`. A design comparison without defects puts one row per option. For a prompt review, the Finding quotes the affected text and the Consequence states its behavioral effect. Under `## Checks`, say what ran, its result, and what stayed unassessed.

### Results

Give one metric table per split, with development and held-out results kept apart. Link the JSON or JSONL results in `dev/` and `held-out/` instead of pasting them. Keep baseline and candidate comparable. Charts label units, sample size and uncertainty.

### Pending

Add one row per open item, in plain words:

| Item | Conflict or blocker | Who acts |
|---|---|---|

Include a message or lease identifier only when another agent must act on it.

## Presentation

Use the [shared format guide](references/report-output.md#choose-the-format), and load its Mermaid or widget reference only for that syntax. After delivery, follow the [HTML offer](references/presentation.md#html-delivery). Use the [report commands](references/report-output.md#render-and-retrieve) for accepted rendering and for `--manifest` discovery.
