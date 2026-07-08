# Research Playbook

Load when the RFC needs evidence. This file describes what evidence the RFC needs; `octocode-research` owns how Octocode research is run.

Use `octocode-research` if installed. If it is missing, use https://github.com/bgauryy/octocode/tree/main/skills/octocode-research or install it with:

```bash
npx octocode skill --name octocode-research
```

Do not copy Octocode router/tool rules into this skill. Ask `octocode-research` for the needed surfaces, citations, confidence, and unresolved gaps, then write the RFC artifacts from that claim ledger.

## Research plan — run only the tracks that matter

| Scenario | Research tracks |
|---|---|
| Existing-system change | Local current state + local blast radius; external prior art if options are unclear |
| New RFC with no handoff | Ask to use `octocode-brainstorming` first when available; if continuing, delegate local/external proof to `octocode-research` |
| Greenfield choice | External prior art + package/repo comparison; local constraints if repo exists |
| Migration | Local current state + contracts/data flows + external migration examples |
| Library/package adoption | npm/package metadata + repo source + local integration points |
| Refactor plan | Local structure + LSP references/callers + AST duplication/smell checks |
| RFC validation | Map each claim to local/external evidence; mark confirmed/likely/uncertain |
| Closing open questions (IMPLEMENTATION.md) | Ask `octocode-research` to resolve each question with local/external/history evidence; a resolution without a citation is not resolved. |

For new RFC research without a brainstorming handoff, ask `octocode-research` to cover the relevant local surface before writing. Add external package, GitHub, history, and docs evidence when prior art matters.

## Evidence rules

- Local claims need `file:line`.
- External code claims need GitHub file path/line or PR/commit link.
- Snippets are leads; ask `octocode-research` to upgrade them before citing.
- Key recommendations need at least one supporting source and one counterpoint or rejected alternative.

## Recovery

| Situation | Move |
|---|---|
| Local search empty | broaden search, inspect structure, try symbols/AST variants |
| GitHub search empty | use repo structure/path search, known files, or clone |
| No external prior art | say so; rely on local constraints and unresolved questions |
| Evidence conflicts | present conflict and decision rule |
| Scope too broad | split into multiple RFCs or phases |
| Two attempts fail | summarize what is known and ask for direction |
