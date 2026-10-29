# Output

<!-- report-contract {"statuses":["Confirmed","Likely","Refuted","Open"],"required":["Answer","Scope","Evidence","Gaps","Checks"],"anyOf":[]} -->

Task content only; no probe output or process metadata. Plain text or Markdown with source links; tables only to compare evidence.

## Response

BLUF with Claim–evidence: answer and confidence first, then each claim beside its exact source anchor, limits (material coverage gaps), next check only if needed. Add by result kind:

| Result | Adds |
|---|---|
| Decision | verdict, risk, verification |
| Root cause | cause, affected contract, fix, verification |
| Refactor | mode and tier, invariants, changes, and verification commands with exit codes |

## Saved result

Reports: `<workspace>/.octocode/octocode-research/<YYYY-MM-DD>-<slug>.md`; intermediate files in its `tmp/`. `<workspace>` = repository root, else the current directory, never a folder inside `.octocode/`. A user-named path wins; a scratch folder such as `.octocode/tmp/` does not (save here, link from scratch notes).

## Presentation choices

Answer → scope → claim-linked evidence → gaps → checks. Separate observed behavior from inference.

| Format | Use when / why |
|---|---|
| `flowchart` / `sequenceDiagram` | Trace a verified dependency path / runtime interaction, with source anchors nearby. |
| `timeline` / `gitGraph` | Explain relevant chronology / branch history supported by repository evidence. |
| Evidence table / `graph` | Compare conflicting sources / inspect a larger relationship network; label unconfirmed edges. |

Use the [shared format guide](references/report-output.md#choose-the-format); load its Mermaid or widget reference only for that syntax.

## Saved report

Save when the result benefits from navigation or later retrieval, such as a diagram, evidence comparison, or several findings. Save every result when the user or project instructions say so; otherwise a concise answer can stay in chat.

Use the [shared header](references/report-output.md#report-header). Accepted `status` values: `Confirmed`, `Likely`, `Refuted`, `Open`.

Follow the header with the `#` title and a lead paragraph that states the answer. Always include, in this order, `## Answer` (the direct answer and its confidence), `## Scope` (repositories, refs, and paths covered), `## Evidence` (Claim–evidence: each claim beside its exact source anchor; only what the sources show), `## Gaps` (Facts–assumptions–unknowns: unread or unproven parts, and any assumption marked as one), and `## Checks` (what ran and its result). Add `## Next` only when something is pending.

After delivery, follow the [HTML offer](references/presentation.md#html-delivery). Use [report commands](references/report-output.md#render-and-retrieve) for accepted rendering and `--manifest` discovery.
