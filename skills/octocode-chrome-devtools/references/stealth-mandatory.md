# Stealth (mandatory)

Load when stealth must be verified, tuned, or deliberately disabled. Why: patches only apply before navigation, and a failed verify aborts the run.

Every `cdp-sandbox.mjs` / `cdp-runner.mjs` run applies `undercover.mjs` **before** your script’s `run(cdp)` unless disabled.

| Rule | Detail |
|------|--------|
| **Default** | `applyMandatoryStealth` + `verifyStealth` (15 checks) — fail run on any `STEALTH_FAIL` |
| **Opt-out** | `CDP_NO_STEALTH=1` or `cdp-runner.mjs --no-stealth` (debug only) |
| **Debug only** | `CDP_STEALTH_ALLOW_FAIL=1` to log failures without exit; `CDP_SKIP_STEALTH_VERIFY=1` to skip verify |
| **Navigation** | `--new-tab https://…` opens `about:blank` first, patches, verifies, then navigates |
| **Attached tab** | http(s) tabs get `Page.reload` after patches so injections apply (`CDP_STEALTH_NO_RELOAD=1` to skip) |

Implementation: `scripts/mandatory-stealth.mjs` (imported from `cdp-runner.mjs`).

`octocode-scraping` `--provider cdp` uses the same patches + verify in its generated runner (unless `--no-cdp-stealth`).

Examples: `scripts/cdp-checks/stealth-check.mjs`, `scripts/cdp-checks/affiliates-stealth-probe.mjs`.
