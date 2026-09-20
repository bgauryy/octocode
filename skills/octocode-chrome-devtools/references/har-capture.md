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

Token budget: summary <2KB; page 10–50 HAR rows; search `.octocode/tmp/chrome-devtools/` before re-browser; `prune-artifacts.mjs` for retention. For several unread response bodies, bridge them into one scrape session and use `semantic-assess-local`: it sends their ordinary read queries as SemanticQuery resources, follows automatic page-local `next.assess` continuations, and retains partial/insufficient/error pages. Do not paste an enormous HAR or infer from its first page. A semantic answer only routes attention; read exact retained spans for proof.

## Bridge
Same scrape `sessionId` + one CDP port → `har-ingest-to-scrape` → `corpus-run-local --regex`. These bridges require the optional `octocode-scraping` skill beside this folder or an explicit `--scraping-skill-dir <dir>`. Thin pages: trust processed API bodies over clean markdown. Playbook: scraping skill `browser-scraping`.

Next: `cdp-checks.md`, `recovery.md`.
