# Output

<!-- report-contract {"statuses":["Pass","Fail","Partial"],"required":["Finding","Steps","Gaps"],"anyOf":[]} -->

## Response

Markdown, task content only. BLUF: the supported finding first, each claim beside its source URL or capture path (Claim–evidence), then the observed post-action state and the coverage gap. No probe output or process metadata.

## Saved result

Captures keep their returned formats (JSON, images) in the runtime folder (normally `<workspace>/.octocode/octocode-chrome-devtools/tmp/`); link large ones, never paste. Reports go to `<workspace>/.octocode/octocode-chrome-devtools/<YYYY-MM-DD>-<slug>.md`; `<workspace>` is the repository root, else cwd, never inside an existing `.octocode/`. A user-named path wins; harness or subagent scratch (`.octocode/tmp/`) does not: save here, link from scratch.

## Presentation choices

Finding → reproducible steps → observed result → gaps. Keep each screenshot beside the claim it demonstrates.

| Format | Use when / why |
|---|---|
| Markdown images | Show UI evidence with meaningful alt text, viewport context and a nearby takeaway; link full captures. |
| `sequenceDiagram` / `stateDiagram-v2` | Explain browser/network ordering / visible state changes. |
| `chart` line or bar / linked HAR or JSON | Compare measured timings / retain machine-readable evidence without pasting large traces. |

Use the [shared format guide](references/report-output.md#choose-the-format); load its Mermaid or widget reference only for that syntax.

## Saved report

Save a result with a table, diagram, 40+ lines, or five findings, or when the user or project instructions ask; short answers stay in chat.

Use the [shared header](references/report-output.md#report-header). Accepted `status` values: `Pass`, `Fail`, `Partial`.

Then a `#` title and a BLUF lead paragraph. Always include `## Finding`, `## Steps`, and `## Gaps` (write `None.` when nothing is missing). Steps is one table:

| # | Action | Observed state | Capture | Result |
|---|---|---|---|---|

Result: `Pass`, `Fail`, or `Partial`. Capture files (screenshots, HAR, traces): add `captures: <workspace-relative capture folder>` to the header and a `## Captures` section linking each; inline-only captures get `inline only` in the Capture cell.

Unfinished work (`next` set) is a Handoff packet: current state and evidence in `## Finding`, attempts in Steps, blockers in `## Gaps`, next action in `next`, plus constraints and the target, frame, or session ids the next call needs. A blocker that needs someone else's action leads the BLUF as an SBAR: situation, background, assessment, recommendation.

After delivery, follow the [HTML offer](references/presentation.md#html-delivery). Use [report commands](references/report-output.md#render-and-retrieve) for accepted rendering and `--manifest` discovery.
