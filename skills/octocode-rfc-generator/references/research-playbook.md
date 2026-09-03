# Research Playbook

Load when the RFC needs evidence. This file describes what evidence the RFC needs; `octocode-research` owns how Octocode research is run. <!-- style-lint: ignore-line passive-voice -->

Follow `references/octocode.md` to load or install `octocode-research`. Ask it for the needed surfaces, citations, confidence, source inventory, and unresolved gaps; this file owns only the RFC evidence plan.

## Research plan — run only the tracks that matter

| Scenario | Research tracks |
|---|---|
| Existing-system change | Local current state + local affected scope; external prior art if options are unclear |
| New RFC with no handoff | Ask to use `octocode-brainstorming` first when available; if continuing, delegate local/external proof to `octocode-research` |
| Greenfield choice | External prior art + package/repository comparison; local constraints if repository exists |
| Migration | Local current state + contracts/data flows + external migration examples |
| Library/package adoption | npm/package metadata + repository source + local integration points |
| Refactor plan | Local structure + LSP references/callers + AST duplication/smell checks |
| RFC validation | Map each claim to local/external evidence; mark confirmed/likely/uncertain |
| Closing open questions (IMPLEMENTATION.md) | Ask `octocode-research` to resolve each question with local/external/history evidence; a resolution without a citation is not resolved. |

For new RFC research without a brainstorming handoff, ask `octocode-research` to cover the relevant local surface before writing.
Add external package, GitHub, history, and docs evidence when prior art matters.
Put the broad source inventory in `RESOURCES.md`; cite decisive claims inline where they affect the decision.

## Evidence rules

- Local claims need `file:line`.
- External code claims need GitHub file path/line or PR/commit link.
- Snippets are leads; ask `octocode-research` to upgrade them before citing.
- Key recommendations need at least one supporting source and one counterpoint or rejected alternative.

## Recovery

| Situation | Move |
|---|---|
| Local search empty | broaden search, inspect structure, try symbols/AST variants |
| GitHub search empty | use repository structure/path search, known files, or clone |
| No external prior art | say so; rely on local constraints and unresolved questions |
| Evidence conflicts | present conflict and decision rule |
| Scope too broad | split into multiple RFCs or phases |
| Another pass is unlikely to close the gap | summarize what is known and ask for direction |

Next: with the claim ledger filled, compare options, and write the decision through `references/rfc-template.md`; when the change touches existing code load `references/rfc-prerequisites.md` first; carry every still-open question into `references/rfc-implementation.md`.
