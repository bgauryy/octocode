# Sanity check — `ghGetFileContent`

Read a known GitHub file or an exact source region. Use the built CLI or the
MCP tool with the same queries; check the live contract with
`octocode schema ghGetFileContent --view query`.

- Pick one read mode per query: `fullContent`, `matchString` (literal unless
  `regex` is set, or a list), `ranges`, or an `offset`/`length` window in
  `unit` lines or bytes.
- Read a file larger than one page. Walk every `next.*` continuation unchanged
  to the end; the concatenated content equals the source with no gap, overlap,
  or truncation marker.
- Request matches with `contextLines`: every match appears once, and gaps show
  `... [lines A-B not requested] ...` instead of silently dropping lines.
- Read at a branch, tag, and full SHA through `ref`; omitting `ref` reads the
  default branch.
- Check identifiers: `owner`, `repo`, `path`, line numbers, and SHAs match the
  repository exactly.
- Check the errors: a missing path, a wrong-case path, and an inaccessible
  repository each return a typed error with a recovery hint.

```json
{"queries":[{"owner":"bgauryy","repo":"octocode","path":"README.md","unit":"lines","offset":0,"length":40}]}
```
