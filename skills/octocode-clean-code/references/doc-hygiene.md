# Doc Hygiene

Load when reviewing inline comments or documentation files for verbosity, dead prose, or god-doc patterns. Why: dead prose is maintained debt; stale docs diverge and mislead.

## Inline comment rules

| Keep | Remove |
|------|--------|
| Non-obvious invariant or constraint | Syntax narration (what the next line does) |
| External contract or spec reference | Commented-out dead code |
| Known edge case with no obvious fix | TODO with no owner and no ticket |
| License header | Block copied verbatim from elsewhere in the file |

Rule: if reading the code answers the question, the comment adds no value — cut it.

## JSDoc / TSDoc rules

| Keep | Remove |
|------|--------|
| Non-obvious parameter constraint | `@param x — the x value` (type restatement) |
| Return invariant not expressed in the type | `@returns the result` |
| `@throws` with a named error class | Deprecated param docs for removed params |
| `@see` with a live reference | `@deprecated` with no migration path documented |

## Documentation file rules

- One concept per file; ≤ 300 lines.
- README: what + install/use + one example; not an internal API reference.
- ARCHITECTURE.md: layer map + data flow + key constraints; not tutorials.
- API reference: one subsystem's public surface only.
- Cross-link; never duplicate. Duplicated prose diverges and rots.

## What not to do

- Do not add a comment explaining why a comment was removed.
- Do not move junk prose from code into the docs folder.
- Do not pad a short doc to look thorough.

Next: step ends here; return to `references/cleanup-playbook.md` EXCISE phase.
