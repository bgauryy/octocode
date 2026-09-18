# Behavioral prompt construction

Load when a rule leaves the next action or its scope unclear. Use this as a diagnostic, not a required writing template.

“Be efficient with tools” leaves the choice open. “Reuse a schema already read; inspect it again when the tool is unfamiliar or its version changes” decides the next call.

## Questions that clarify a rule

| Question | Add only what is missing |
|---|---|
| What action changes? | A concrete instruction an agent can apply. |
| What nearby behavior causes confusion? | A distinction or small example. |
| Why does it matter? | The consequence that explains the boundary. |
| Where does it apply? | Scope, prerequisites, and exceptions. |
| How is success established? | Evidence appropriate to the claim. |

Merge answers that fit one sentence. Do not add a contrast, rationale, or labeled section when the action is already clear.

## Example: evidence before deletion

Weak: “Delete a symbol when `lspSearch` references returns empty.”

Repair: “Before deleting a symbol, read its exact source, check applicable `lspSearch` references and entrypoint/configuration paths, then run the relevant checks. An empty result only describes that query's completed scope; it does not rule out dynamic imports, shell execution, or external consumers.”

`localSearch` finds lexical candidates; `astSearch` inspects syntax and file topology; `localFetch` establishes the source anchor. LSP resolves symbols within the configured project. Select the evidence needed for the decision; invoking every tool is not a proof requirement. Errors, partial results, and unavailable servers cannot support absence claims.

## Remove redundant instructions

- Keep one owner for a rule and route to its conditional detail.
- Cut repeated preferences, motivational language, role-play, and decorative terminology.
- Leave field types and limits in their schema; see `references/tools/tool-contracts.md`.
- Preserve commands, identifiers, authority boundaries, output shapes, and working exceptions.

Judge density by the decisions a sentence enables. Report reliability improvements only when a suitable evaluation measured them.

Next: use `references/writing/attention.md` for placement, `references/writing/conciseness-toolkit.md` for cuts, or `references/flow/fix.md` to apply the repair.
