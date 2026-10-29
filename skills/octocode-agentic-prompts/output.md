# Output

<!-- report-contract {"statuses":["Ready","Needs work"],"required":[],"anyOf":[["Findings","Changes"]]} -->

Deliver task content only; no probe output or process metadata.

## Response

Prompts in the requested format. Edit: full replacement, change summary, verification. Review: affected text, behavioral effect, repair.

A standalone review or new prompt with no destination goes to `<workspace>/.octocode/octocode-agentic-prompts/<YYYY-MM-DD>-<slug>.md`; a user-named path wins; a small chat edit needs no file.

## Presentation choices

Instruction source → behavioral effect → replacement → verification. Keep replacement text copyable in its own language-tagged fence.

| Format | Use when / why |
|---|---|
| `flowchart` / `sequenceDiagram` | Trace loaders, instruction precedence or tool interactions to explain where behavior changes. |
| Short comparison table / `diff` fence | Compare old and new rules; put long findings in linked subsections. |
| `chart` bar | Compare measured baseline/candidate outcomes with the same evaluation conditions; omit invented severity scores. |

Use the [shared format guide](references/report-output.md#choose-the-format); load its Mermaid or widget reference only for that syntax.

## Saved report

Save when the result has a table, a diagram, over ~40 lines or five findings, or the user asks. Keep every result when user or project instructions say so.

Use the [shared header](references/report-output.md#report-header). Accepted `status` values: `Ready`, `Needs work`.

Open the `#` title with a lead paragraph that states the answer.

Put findings under `## Findings` as one table, most severe first, with `Critical`, `High`, `Medium`, or `Low` severity. When a rewrite was requested, give the full replacement text in a fenced block under `## Replacement`:

| Location | Text | Behavioral effect | Severity | Repair |
|---|---|---|---|---|

A change report has `## Changes`, one row per edit; every saved report has either `## Findings` or `## Changes`.

After delivery, follow the [HTML offer](references/presentation.md#html-delivery). Use [report commands](references/report-output.md#render-and-retrieve) for accepted rendering and `--manifest` discovery.
