# Chrome DevTools Skill

Live Chrome/CDP evidence for DOM actionability, network, console, performance, storage, HAR, screenshots, and authenticated pages. Use [`octocode-scraping`](https://github.com/bgauryy/octocode/tree/main/skills/octocode-scraping) for static crawl or bulk extraction.

## Install

```bash
npx octocode skill --name octocode-chrome-devtools
```

Requires Chrome and Node 22+; sandbox `--allow-net` needs Node 25+. Scripts use Node built-ins plus the vendored config module, so no package install is needed.

Provide a URL, expected behavior, and the signal to inspect. The agent opens or attaches to one CDP port, applies/verifies stealth, runs one focused check, reuses the tab, and queries saved measure/HAR/corpus artifacts before reopening Chrome.

Ask first for real-profile access, cookie transfer, CAPTCHA/MFA, or destructive/real-data mutations. Secrets remain redacted.

```bash
SKILL_DIR="$(npx octocode skill dir octocode-chrome-devtools)"
node "$SKILL_DIR/scripts/open-browser.mjs" --headless --port 9222
node "$SKILL_DIR/scripts/cdp-sandbox.mjs" --list-targets --port 9222
```

Agent behavior: `SKILL.md` and `references/`. Ready checks: `references/cdp-checks.md`.
