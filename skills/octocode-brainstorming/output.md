# Output

<!-- report-contract {"statuses":["Recommended","Open","Rejected"],"required":[],"anyOf":[]} -->

## Response

Lead with the recommendation and confidence, then the deciding evidence, strongest objection and useful next check. A short answer needs no table, ledger or effort announcement.

- Cite deciding claims at the supporting sentence with exact verified URLs. Label inference, secondary evidence and unchecked scope.
- Include dates, source context and applicability limits when they affect the conclusion. Check numbers, dates and claimed benefits against the evidence.
- Explain [confidence](references/critical-thinking.md#calibrated-confidence); never invent probabilities. Report changed or unresolved hypotheses when consequential.
- State the recommendation as plain claims, checked against [writing tells](references/writing-tells.md).
- For a costly or irreversible choice, end with the decision the user must make and the deciding source to check themselves.
- Include verification counts only when they explain a material evidence gap. No routine roster, state dump or repeated source metadata.

A `Sources` list can collect references when useful; adjacent citations suffice for a short answer. With no verified evidence, say so.

## Checkpoints

Keep a recovery note only when interruption or handoff is likely: current question, supported interim answer, deciding evidence paths, unresolved choice and next action. Add hypotheses or ledger paths only when used. Use the run's existing notes rather than creating a second state record; without one, use `<workspace>/.octocode/octocode-brainstorming/tmp/<run>/state.md`.

Continue authorized work; pause dependent work only for a missing decision or explicit user stop.

## Think together

Show the user's view, options labeled by origin, and the question most likely to change the choice. Separate known facts, assumptions and researched evidence.

## Presentation choices

Use a comparison table for alternatives sharing criteria, a diagram for useful structure, and prose for a short result. Explain plotted scales and estimates. Follow the [shared format guide](references/report-output.md#choose-the-format) only for the selected format; load specialized syntax on demand.

## Saved report

Save when requested, required by the project, or useful for a durable decision or handoff. A short comparison can stay in chat. Use the user-named path; otherwise `<workspace>/.octocode/octocode-brainstorming/<YYYY-MM-DD>-<slug>.md`, with `<workspace>` the repository root or current directory, never an existing `.octocode/` folder. Scratch is not the final destination.

Write a title, recommendation and supporting evidence; add alternatives, dissent, limits and next action where they carry information. Link ledgers only when they exist. No fixed section order or empty required sections.

For catalog discovery or HTML rendering, use the [shared header](references/report-output.md#report-header) with `status` and `question`; these fields serve the catalog, not ordinary replies. Accepted `status` values: `Recommended`, `Open`, `Rejected`.
Use [report commands](references/report-output.md#render-and-retrieve) for accepted rendering or `--manifest` discovery. Follow the [HTML delivery guidance](references/presentation.md#html-delivery) when that view is useful.
