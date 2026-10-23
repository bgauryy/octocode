Load when a live browser step joins a scrape (auth, clicks, JS-only page, HAR, perf).

## Session handles

- One sessionId and one CDP port (`--port` plus `--keep-tab`) keep evidence joinable; `octocode-chrome-devtools` owns live CDP, this skill owns the corpus.
- CDP artifacts: `.octocode/octocode-chrome-devtools/tmp/<run>/`, passed to `har-ingest.mjs --from-cdp-dir`.
- Run the CDP provider from the intended workspace. Browser state, temporary runners and captures belong to that cwd; the installed executable may live elsewhere. A port is a connection handle, not a durable research-session identity.

## Playbook

1. Map: `fetch.mjs` → corpus, `AGENT_INDEX.json`, `graph/graph.json`.
2. Packet: `har-ingest.mjs --export-packet` → `extracts/bridge-handoff.json`.
3. Live DOM: chrome `page-snapshot` → `dom-operations-check` (`--keep-tab`). Static only: `dom-find.mjs`.
4. Click or fill: same tab, after verifying actionability; only user-authorized actions. Inspect uncertain results before retrying. Zero actionability rows: chrome `actionability-diagnostics`.
5. Measure: `performance`/`network`/`storage-measure-check` with `MEASURE_EXISTING=1`.
6. Query: `measure-query --dir|--latest`; HAR → `har-pager`.
7. Process: `har-ingest.mjs --session-dir <session> --from-cdp-dir <run>` (chrome alias `har-ingest-to-scrape`).
8. Prove: read deciding spans or `corpus-run.mjs --roots cdp,extracts --regex …` (alias `corpus-run-local`); never re-open the browser for the same body. A semantic answer is not proof.

- Emit paths and counts, never cookies or tokens.

## `--provider cdp`

- Wait for readiness: `fetch.mjs --url <u> --provider cdp --wait-for <selector> --wait-text <text> --cdp-wait-ms 15000`.
- `--cdp-stealth`: explicit emulation experiment only; native is default.
- Clicks, event waits, or adaptive decisions: Chrome's combined executor, then `har-ingest.mjs`.
- Still thin or blocked: [recovery](recovery.md).
