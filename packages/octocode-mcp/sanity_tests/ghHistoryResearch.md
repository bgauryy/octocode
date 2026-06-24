# Sanity check - `ghHistoryResearch`

> Search pull requests or inspect commit history.
>
> **Manual runtime checklist** - run each step against the live MCP tool (or `benchmark/github/scripts/call-tool.mjs ghHistoryResearch '<queries-json>'`) and tick the box.
> Automated schema-contract checks live in `tests/tools/all-tools.pagination-contract.test.ts`; cursor-uniformity in `tests/tools/all-tools.pagination.test.ts`.

## 1. Scheme
- [ ] Tool is registered and accepts the bulk envelope: `queries[]`, `responseCharOffset`, `responseCharLength`.
- [ ] `type` accepts exactly `prs` or `commits`.
- [ ] PR mode accepts `keywordsToSearch`, `prNumber`, `page`, and `limit`.
- [ ] Commit mode accepts `path`, `since`, `until`, `page`, and `perPage`.
- [ ] A minimal valid query parses (see Example).

## 2. Pagination
- [ ] Run a broad PR or commit query with a small page size: the response is bounded and exposes follow-up pagination.
- [ ] Walk the cursor/page to completion; every reported item is reachable.
- [ ] No clipped/truncated marker appears without an explicit continuation.

## 3. Quality
- [ ] PR rows include stable repo, number, title, and URL identifiers.
- [ ] Commit rows include stable SHA, path context where requested, and date ordering.
- [ ] Deep PR reads return requested comments/patch detail without changing `type` semantics.

## 4. Agent Effectiveness
- [ ] `concise:true` produces a smaller list view for triage.
- [ ] `type:"commits"` plus `path` can answer file-history questions without a PR detour.
- [ ] Output preserves enough identifiers for follow-up `ghGetFileContent` or OQL materialization.

## Example call
```json
{ "queries": [ { "type": "prs", "owner": "facebook", "repo": "react", "keywordsToSearch": ["useState"], "limit": 5 } ] }
```
