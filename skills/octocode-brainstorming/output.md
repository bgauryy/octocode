# Output

<!-- report-contract {"statuses":["Recommended","Open","Rejected"],"required":[],"anyOf":[]} -->

## Response

Lead with the recommendation and confidence, then the deciding claims, strongest objection and useful next check. No effort announcement.

- Cite deciding claims at the supporting sentence with exact URLs of [validated sources](references/verification.md#validated-sources) only. An unvalidated lead appears only as a named gap. Label inference, secondary evidence and unchecked scope.
- Give each deciding claim its [status](references/proof-bar.md#contradict-and-prove) (`proven`, `contradicted`, `unresolved`) and say which disconfirming searches it survived. Several claims or options sharing criteria read best as a table: option or claim, origin, status, supporting sources and, in the same row, the sources against it or its limits.
- Include dates, source context and applicability limits when they affect the conclusion. Check numbers, dates and claimed benefits against the evidence.
- Explain [confidence](references/proof-bar.md#calibrated-confidence); never invent probabilities. Report changed or unresolved hypotheses when consequential.
- State the recommendation as plain claims, checked against [writing tells](references/writing-tells.md).
- For a costly or irreversible choice, end with the decision the user must make and the deciding source to check themselves.
- When judging an existing plan or artifact, separate what the evidence says to keep, change and drop.
- When a search vendor failed in a way the user can fix (`keyRejected`, `quotaExceeded`), say so in one line with the `fix` that `search.mjs` printed; omit vendors that worked.
- Include verification counts only when they explain a material evidence gap. No routine roster, state dump or repeated source metadata.
- End with named gaps: claims, methods or sources not researched or not read, so missing evidence is not mistaken for evidence of absence.

A `Sources` list can collect validated references when useful; adjacent citations suffice for a short answer. With no validated evidence, say so and give the gaps instead of a recommendation that looks supported.

## Checkpoints

Keep a recovery note only when interruption or handoff is likely: current question, supported interim answer, deciding evidence paths, unresolved choice and next action. Add hypotheses or ledger paths only when used. Use the run's existing notes rather than creating a second state record; without one, use `<run>/state.md`.

When to pause or resume work: [recovery table](references/phases.md#understand-and-diverge).

## Think together

Show the user's view, options labeled by origin (user, yours, found), and the question most likely to change the choice. Separate known facts, assumptions and researched evidence.

## Presentation choices

Beyond the comparison table the [response](#response) uses, add a diagram only for structure the text cannot show, and keep a short result in prose. Explain plotted scales and estimates. Follow the [shared format guide](references/report-output.md#choose-the-format) only for the selected format; load specialized syntax on demand.

## Saved report

Save when requested, required by the project, or useful for a durable decision or handoff. A short comparison can stay in chat. Use the user-named path; otherwise `<workspace>/.octocode/octocode-brainstorming/<YYYY-MM-DD>-<slug>.md`, with `<workspace>` the repository root or current directory; create `.octocode/octocode-brainstorming/` there if needed, and never treat a `.octocode/` folder itself as the workspace. Scratch is not the final destination.

Write a title, recommendation and supporting evidence; add alternatives, dissent, limits and next action where they carry information. Link ledgers only when they exist. No fixed section order or empty required sections.

A saved report starts with the [shared header](references/report-output.md#report-header) (`status`, `question`): the contract check run after each save requires it, and the catalog and HTML use it. Ordinary replies need no header. Accepted `status` values: `Recommended`, `Open`, `Rejected`.
Use [report commands](references/report-output.md#render-and-retrieve) for accepted rendering or `--manifest` discovery. Follow the [HTML delivery guidance](references/presentation.md#html-delivery) when that view is useful.
