# Sanity check — `localFetch`

Use the built CLI or MCP tool with the same queries.

- Inspect full and compact schemas: the name is `localFetch`; fields include `chunkType`, `offset`, and `chunkSize`.
- Read a UTF-8 fixture containing emoji, CRLF, blank lines, and a long line. Walk every `next.continue` unchanged in both chunk modes and concatenate content; compare with the exact source.
- Request an inclusive line range. Its pages stop at the selected end and contain no injected line numbers.
- Request literal and regex matches with overlapping context. Walk small line/byte chunks; check selected ranges, page-local matching anchors, and no duplicated content.
- Check original-file `totalLines`/`sourceBytes` on every successful page, empty file, and no-match result. Compare view totals and returned bytes separately.
- Exercise `none`, `standard`, and `symbols` with the real native engine. Match reads remain exact and expose `minifyFallback`; unsupported outlines expose their applied view and fallback reason.
- Verify Unicode boundary handling, oversized-line byte recovery, full-content limit recovery, and the secret scanner's terminal limit.
- Inspect both structured and text output: executable hints and typed fallback reasons survive rendering.

```json
{"queries":[{"path":"/ABS/repo/README.md","chunkType":"bytes","offset":0,"chunkSize":1024}]}
```
