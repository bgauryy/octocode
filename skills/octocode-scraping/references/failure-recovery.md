# Failure Recovery

Load when a scrape fails, blocks, times out, or creates too much data. Why: recover predictably without bypassing policy.

| Situation | Fix |
|---|---|
| hosted provider key missing | Run `provider-check.mjs`; ask user to configure env or choose a keyless route. |
| direct `403` / bot block | Try CDP read-only validation/fetch if task needs it; ask before hosted anti-bot escalation. |
| hosted `403` | Wrong key or credits exhausted; stop and report sanitized status. |
| `404` | Verify target URL; try one direct/browser check if in scope. |
| `422` | Invalid option; print sanitized params and remove/fix the bad value. |
| `423` anti-bot | Try CDP or hosted browser rendering once with lower rate/selector wait; ask before stronger escalation. |
| Timeout | Narrow to one URL, add `--wait-for`, or retry once with smaller limits. |
| Huge output | Use `--max-raw-bytes`, `--max-text-bytes`, or `--no-raw`; search compact files first. |
| Auth required | Stop and route to `octocode-chrome-devtools`; ask before cookie/profile use. |
| CAPTCHA/MFA | Stop and ask; do not bypass. |

After two same-class failures, stop and summarize evidence, attempted route, sanitized status, and next approval needed.
