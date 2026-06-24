# Sanity check - `localBinaryInspect`

> Inspect archives, compressed streams, native binaries, and embedded strings.
>
> **Manual runtime checklist** - run each step against the live MCP tool (or `benchmark/github/scripts/call-tool.mjs localBinaryInspect '<queries-json>'`) and tick the box.
> Automated schema-contract checks live in `tests/tools/all-tools.pagination-contract.test.ts`; cursor-uniformity in `tests/tools/all-tools.pagination.test.ts`.

## 1. Scheme
- [ ] Tool is registered and accepts the bulk envelope: `queries[]`, `responseCharOffset`, `responseCharLength`.
- [ ] `mode` exposes the current enum: `inspect`, `list`, `extract`, `decompress`, `strings`, `unpack`.
- [ ] Mode-specific fields are documented: archive entry paths for extract/list, `minLength` and char windows for strings, and destination handling for unpack/extract.
- [ ] A minimal valid query parses (see Example).

## 2. Pagination
- [ ] Archive listing supports entry pagination when the archive is large.
- [ ] `strings` supports char-window continuation for large extracted text.
- [ ] Extraction/unpack returns concrete local locations for follow-up local/OQL searches.

## 3. Quality
- [ ] `inspect` identifies format/architecture/imports/exports where available.
- [ ] `list` preserves archive paths and file metadata.
- [ ] `strings` finds ASCII and UTF-16 strings without shelling out to platform tools.

## 4. Agent Effectiveness
- [ ] Output gives follow-up paths suitable for `localGetFileContent`, `localSearchCode`, or OQL.
- [ ] Unsupported formats return typed errors, not empty success.
- [ ] Secret redaction still applies to text and structured content.

## Example call
```json
{ "queries": [ { "path": "archive.zip", "mode": "list", "entriesPerPage": 50 } ] }
```
