# HAR capture and data Replay

Load for HAR export, API replay, or token budget. Why: evidence in files; secrets out of chat.

| Need | Use |
|---|---|
| Live while user acts | CDP monitor |
| API forensics | Network events / measure → query |
| Huge capture | HAR + `har-pager` + `har-redact` |
| One body by `requestId` | `Network.getResponseBody` while cached, or `network-body-har-fetch-check` |
| WebSocket | `intents-inspect` websocket — not HAR |

## Rules
Write under `cdp.outputDir`; stdout = counts + `[ARTIFACT]`. Page: `har-pager.mjs` (`--filter/--kind/--status/--url-regex`). Share: `har-redact.mjs`. Prefer measure trio + `measure-query` before long monitors. `Network.enable` covers all frames; HAR is HTTP or HTTPS only.

```bash
node <skill>/scripts/cdp-checks/har-pager.mjs live-network.har --filter failures --page 1
node <skill>/scripts/cdp-checks/har-redact.mjs live-network.har --strip-bodies
```

Token budget: summary <2KB; page 10–50 HAR rows; search `.octocode/tmp/chrome-devtools/` before re-browser; `prune-artifacts.mjs` for retention.

## Bridge
Same scrape `sessionId` + one CDP port → `har-ingest-to-scrape` → `corpus-run-local --regex`. Thin pages: trust processed API bodies over clean markdown. Playbook: scraping skill `browser-scraping`.

Next: `cdp-checks.md`, `recovery.md`.
