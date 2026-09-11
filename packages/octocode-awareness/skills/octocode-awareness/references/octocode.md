# Octocode research operations

Load when Awareness needs code, repository, package, history, or skill evidence. Awareness owns coordination and memory; Octocode research tools own source discovery and inspection. This skill does not bundle an Octocode binary.

Use `octocode-research` when available. Otherwise use the current MCP tools or CLI directly. In the monorepo, use the built CLI; elsewhere use `npx -y octocode`. Inspect an unfamiliar tool schema once and reuse it until the contract changes:

```bash
npx -y octocode tools localSearch astSearch localFetch lspSearch --scheme
```

| Question | Tool |
|---|---|
| Text or regex occurrence | `localSearch`; no `operation` field |
| Paths, syntax, symbols, or file topology | `astSearch`: `files`, `tree`, `match`, `symbols`, or `topology` |
| Exact source or a deliberate transformed view | `localFetch`; exact by default, choose minification explicitly |
| Symbol identity or uses | `lspSearch` with the operation's real anchor and scope |
| Remote discovery and history | `ghSearch`, `ghSearchHistory` |
| Exact remote content or history item | `ghGetFileContent`, `ghGetHistoryItem` |
| Local checkout or package lookup | `ghCloneRepo` when enabled; `artifactSearch` |

Choose calls that answer the question. Read exact source before anchored LSP; `lineHint` is 1-based, while `position` is 0-based UTF-16. Follow returned executable continuations and distinguish empty, partial, and error results. A completed empty query describes its scope; it cannot rule out dynamic or external consumers.

Carry decisive paths, lines, revisions, and evidence limits into a signal or memory only when they change a coordination decision. A peer report remains a lead until verified.

Next: return evidence to the owning coordination decision; use `references/output-routing.md` when choosing where to record it.
