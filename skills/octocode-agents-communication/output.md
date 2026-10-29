# Output

<!-- report-contract {"statuses":["Resolved","Waiting","Blocked"],"required":["Outcome","Pending"],"anyOf":[]} -->

Task content only, never probe output or process metadata; plain text or Markdown (`/cli <command> --json` for machine-readable output).

## Response

BLUF: Outcome first, then any pending conflict and who must act; message and lease identifiers stay in the protocol.

## Saved result

Report path: `<workspace>/.octocode/octocode-agents-communication/<YYYY-MM-DD>-<slug>.md` (`<workspace>` = repository root, else current directory; never a folder inside an existing `.octocode/`). A user-named path wins; a harness or subagent scratch folder does not (save here, link from scratch notes).

## Presentation choices

Outcome → unresolved conflict → responsible actor and next action. Keep protocol identifiers only where someone needs them to act.

| Format | Use when / why |
|---|---|
| `sequenceDiagram` | Explain message order, acknowledgements and retries. |
| `swimlane-beta` | Clarify responsibility and handoffs across agents. |
| Pending-item table | Make blockers and ownership actionable; link detailed records instead of pasting logs. |

Use the [shared format guide](references/report-output.md#choose-the-format); load its Mermaid or widget reference only for that syntax.

## Saved report

The caller-selected database, not the report, holds messages and evidence; a chat summary naming `activity`, `locks`, or `view` usually suffices. Save a report when the outcome spans several agents or open conflicts, the user asks for multi-agent status, or instructions (e.g. `AGENTS.md`) say to keep every result.

Use the [shared header](references/report-output.md#report-header). Accepted `status` values: `Resolved`, `Waiting`, `Blocked`.

Then a `#` title and a lead paragraph stating the answer. Always include `## Outcome` and `## Pending` (write `None.` when nothing is open), with one row per open item:

| Item | Conflict or blocker | Who acts |
|---|---|---|

Plain-word item names; a message or lease identifier only when another agent must act on it.

After delivery, follow the [HTML offer](references/presentation.md#html-delivery). Use [report commands](references/report-output.md#render-and-retrieve) for accepted rendering and `--manifest` discovery.
