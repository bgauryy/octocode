# Browser Scraping

Load when the target needs live page state, auth, interaction, screenshots, network evidence, or CDP. Why: `octocode-chrome-devtools` already owns browser automation safety.

## Handoff
Use `octocode-chrome-devtools`; load that skill's automation route and script patterns as needed. It owns two capabilities relevant to bot-walled targets — ask for them by name rather than re-deriving CDP calls:
- **Stealth**: its stealth-patch helper (apply/verify pair) — apply before navigating a site likely to fingerprint headless Chrome; self-test before trusting the fetch.
- **Human-like input**: its trusted-input helper — CDP `Input.*` event sequences (Bezier mouse, WPM typing) for targets with behavioral anti-bot checks, not just fingerprint checks.

Do not reimplement CDP/stealth logic here — `octocode-scraping`'s `--provider direct`/`scrapingant` are single-shot fetches; CDP automation is stateful and belongs on the other side of this handoff.

## Read-only CDP scraping rules
- Lock target, trigger, readiness signal, and evidence prefixes before scripting.
- Attach listeners before navigation; use `about:blank` then `Page.navigate` for new loads.
- Use visible/enabled selectors and smart waits.
- Emit counts and sample rows; write large DOM/HAR/output under `.octocode/tmp/scrape/{sessionId}/` or the CDP output dir.
- Never print cookies, tokens, session IDs, or localStorage secrets.

## Ask first
Real profile, cookie bridge, CAPTCHA/MFA, destructive writes, form submission with real user data, purchases, sends, deletes, or account changes require explicit user approval.
