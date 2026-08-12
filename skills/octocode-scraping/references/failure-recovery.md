# Failure Recovery

Load when a scrape fails, blocks, times out, or creates too much data. Why: recover without bypassing policy or burning credits.

| Situation | Fix |
|---|---|
| hosted key missing | Run `provider-check.mjs`; use keyless `direct`/`cdp`, or ask user before configuring `SCRAPING_ANT`. |
| direct `403` / bot block | Try `--provider cdp` once; **ask** before `--provider scrapingant`. |
| hosted `403` | Wrong key or credits exhausted; stop; sanitized status only. |
| `404` | Verify URL; one direct/cdp check if in scope. |
| `422` | Invalid option; print sanitized params; fix/remove. |
| `423` anti-bot | One CDP or hosted browser attempt with lower rate/`--wait-for`; ask before stronger escalation. |
| thin-200 / JS shell | Evidence in corpus; escalate to CDP actionability/diagnostics; not automatic hosted. |
| Timeout | One URL, `--wait-for`, or smaller limits; retry once. |
| Huge output | `--max-raw-bytes` / `--max-text-bytes` / `--no-raw`; search compact files first. |
| Auth required | Stop → `octocode-chrome-devtools`; ask before cookie/profile. |
| CAPTCHA/MFA | Stop and ask; do not bypass. |

After two same-class failures: stop; summarize evidence, route tried, sanitized status, next approval.

**Coverage note:** hermetic evals + optional `OCTOCODE_LIVE_BENCH=1` (example.com/httpbin) do not replace this table for real bot-walls/regions.
