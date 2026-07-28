# Browser Scraping

Load when static graph candidates need live validation or interaction. Why: `octocode-chrome-devtools` owns CDP safety, actionability, cookies/storage, screenshots, and network proof.

## Static → CDP → corpus loop
1. Use this skill first for public/static fetch, corpus, and graph candidates.
2. Hand candidate URLs/selectors/actions to `octocode-chrome-devtools` for live checks: visible/enabled/clickable, search input, pagination button, menu reveal, infinite scroll, cookies/storage, network, screenshot, auth-gated state.
3. If CDP finds 0 actionability rows, run `actionability-diagnostics.mjs` to classify blocked, JS-shell, selector-mismatch, consent-region, or timing-hydration.
4. Save CDP outputs under `.octocode/tmp/scrape/{sessionId}/cdp/` or the CDP output dir.
5. Feed newly discovered URLs, API endpoints, or saved DOM/text back into this corpus and continue `session-corpus.md` proof search.

## Handoff packet
```json
{
  "url": "https://site/page",
  "intent": "validate actionability and discover resulting URL/data",
  "selectors": ["form[role=search]", "button[type=submit]", "a[rel=next]"],
  "evidencePrefix": "[SCRAPE_GRAPH]",
  "outputDir": ".octocode/tmp/scrape/<session>/cdp"
}
```

## CDP rules
- Attach listeners before navigation; use `about:blank` then `Page.navigate` for new loads.
- Use smart waits and visible/enabled selectors.
- Emit counts, sample rows, new URLs, screenshots/HAR paths, and storage counts — not raw secrets.
- Never print cookies, tokens, session IDs, or localStorage values.

## Ask first
Real profile, cookie bridge, CAPTCHA/MFA, destructive writes, form submission with real user data, purchases, sends, deletes, or account changes require explicit user approval.
