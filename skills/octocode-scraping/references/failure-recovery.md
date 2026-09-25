# Failure Recovery

Load when a scrape fails, blocks, times out, or creates too much data. Why: recover without bypassing policy or burning credits.

| Situation | Fix |
|---|---|
| hosted key missing | Run `provider-check.mjs`; use keyless `direct`/`cdp`, or ask user before configuring `SCRAPING_ANT`. |
| direct `403` / bot block | Follow output `next` and try one Chrome diagnostic; **ask** before `--provider scrapingant`. |
| hosted `403` | Wrong key or credits exhausted; stop; sanitized status only. |
| `404` | Verify URL; one direct/cdp check if in scope. |
| `422` | Invalid option; print sanitized params; fix/remove. |
| `423` anti-bot | One CDP or hosted browser attempt with lower rate/`--wait-for`; ask before stronger escalation. |
| thin-200 / JS shell | Output includes `next`; capture once with Chrome, retain the artifact, and bridge it into the same corpus. Never auto-hosted. |
| robots disallow / unavailable 5xx | Skip that URL/origin. Report the saved failure; do not route around the policy with Chrome. |
| `429` / `503` with `Retry-After` | Direct fetch waits once only when the requested delay is at most 10 seconds; otherwise report it and stop. |
| Timeout | One URL, `--wait-for`, or smaller limits; retry once. |
| Huge output | `--max-raw-bytes` / `--max-text-bytes` / `--no-raw`; search compact files first. Read targeted spans; classification delegates to `octocode-clasify` through `clasify-screen.md`. |
| Auth required | Stop → `octocode-chrome-devtools`; ask before cookie/profile. |
| CAPTCHA/MFA | Stop and ask; do not bypass. |

After two same-class failures: stop; summarize evidence, route tried, sanitized status, next approval.

**Coverage note:** local hermetic checks do not replace this table for real bot-walls/regions.

Next: to re-pick a cheaper route load `references/route-selection.md`; for a live-browser attempt load `references/browser-scraping.md`; only after approved spend load `references/scrapingant.md`.
