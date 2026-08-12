# Chrome DevTools Skill

Live Chrome/CDP evidence for agents: network, console, perf, DOM/actionability, storage, HAR, screenshots, auth-gated pages. Static crawl → [`octocode-scraping`](https://github.com/bgauryy/octocode/tree/main/skills/octocode-scraping).

## Install
```bash
npx octocode skill --name octocode-chrome-devtools
```
Prereqs: Chrome; Node **22+** (sandbox `--allow-net` needs **25+**).

## Ask the agent
Include URL, expected behavior, and the signal you care about. One intent + one CDP port.

- “Debug why submit fails on this page.”
- “Watch network when I click checkout.”
- “Visible browser — I’ll log in, then inspect API errors.”
- “Measure page health (perf/net/storage) and summarize findings.”
- “Capture HAR, then prove the product API field in the scrape corpus.”

Agent loop: `open-browser` → stealth → focused CDP script → prefixed findings → reuse `--keep-tab` → `measure-query` / `har-pager` / bridge before reopening Chrome.

## Safety
Ask first: real profile, cookie bridge, CAPTCHA/MFA, destructive writes. Secrets stay redacted.

## Optional CLI
```bash
SKILL_DIR="$(npx octocode skill dir octocode-chrome-devtools)"
node "$SKILL_DIR/scripts/open-browser.mjs" --headless --port 9222
node "$SKILL_DIR/scripts/cdp-sandbox.mjs" --list-targets --port 9222
node "$SKILL_DIR/scripts/eval-benchmark-suite.mjs"   # hermetic done gate
```

Agent truth: `SKILL.md` + `references/`. Check catalog: `references/cdp-checks.md`.
