# Failure Recovery

Load when a scrape fails, blocks, times out, or creates too much data. Why: recover without bypassing policy or burning credits.

| Situation | Fix |
|---|---|
| hosted key missing | Run `provider-check.mjs`; use a suitable keyless route or the authorized `SCRAPING_ANT` setup. Ask only for missing credentials or authority. |
| direct `403` / bot block | Inspect the returned reason and route. A browser diagnostic may distinguish rendering from access restrictions; respect challenges and policy. Paid escalation needs existing or new spend authority. |
| hosted `403` | Stop the hosted call and inspect sanitized provider status. Verify credentials, credits, and access policy before diagnosing or retrying. |
| `404` | Verify the URL and relevant links; retry only with a reason the resource is reachable. |
| `422` | Invalid option; print sanitized params; fix/remove. |
| `423` anti-bot | Inspect the block and any documented pacing or access requirement. Continue only through an authorized route that respects the restriction. |
| thin-200 / JS shell | Follow the lobby browser route; a hosted provider remains an explicit choice. |
| robots disallow / unavailable 5xx | Skip that URL/origin. Report the saved failure; do not route around the policy with Chrome. |
| `429` / `503` with `Retry-After` | Direct fetch waits once only when the requested delay is at most 10 seconds; otherwise report it and stop. |
| Timeout | Identify whether connection, rendering, or scope caused it. Adjust readiness or collection limits and retry when that can resolve the cause. |
| Huge output | Follow query continuations; use `--chunk-bytes` / `--max-text-bytes` for part sizes. Network caps disclose partial collection; `--no-raw` explicitly excludes raw audit data. |

On a stop, summarize evidence, route tried, sanitized status, and the next useful check or missing authority. Reuse authorization already given.

**Coverage note:** local hermetic checks do not replace this table for real bot-walls/regions.

Next: to re-pick a cheaper route load `references/route-selection.md`; for a live-browser attempt load `references/browser-scraping.md`; only after approved spend load `references/providers.md` § ScrapingAnt.
