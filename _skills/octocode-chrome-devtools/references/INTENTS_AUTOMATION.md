# CDP Automation And Scrape Intent Details

## automate

**Trigger phrases:** "automate", "do X", "click the button", "fill the form", "type into", "submit", "perform this flow", "go through the steps", "interact with", "navigate to X then Y"

**Purpose:** Drive the browser through a multi-step user flow. Use JS evaluation for interactions — it's faster and more reliable than CDP Input events for most cases.

**Domains:** `Page.enable`, `Runtime.enable`, `Network.enable` (add others as needed for observation)


**Interaction toolkit — examples, not a fixed framework:**

Prefer `Runtime.evaluate` for app-level actions when you have stable selectors. Use `Input.*` from `CDP_AGENT_REFERENCE.md` when the task needs realistic pointer/keyboard behavior.

```js
await cdp.send('Runtime.evaluate', {
  expression: `document.querySelector(${JSON.stringify(selector)})?.click()`,
});

await cdp.send('Runtime.evaluate', {
  expression: `(() => {
    const el = document.querySelector(${JSON.stringify(selector)});
    const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set;
    set.call(el, ${JSON.stringify(value)});
    el.dispatchEvent(new Event('input', { bubbles: true }));
    el.dispatchEvent(new Event('change', { bubbles: true }));
  })()`,
});
```

For robust waits, load only the helper you need from `SCRIPT_PATTERNS.md`: `waitForSelector with Actionability` or `waitForNetworkIdle`.

**Flow recipe:**

1. Enable `Page`, `Runtime`, and `Network` if observing requests.
2. Attach error/network listeners before navigation.
3. Navigate or attach to the current tab.
4. Wait for the next actionable element, perform one step, emit `[AUTOMATE]`.
5. Verify the expected outcome, then stop or continue to the next user-requested step.

**Output prefixes:** `[AUTOMATE]` `[FINDING]` `[ACTION]` `[METRIC]`

**`[AUTOMATE]` prefix:** Emit for each step executed: `[AUTOMATE] clicked "${selector}"`, `[AUTOMATE] typed into "${selector}"`, `[AUTOMATE] waited for "${selector}" — found after Nms`

**`[FINDING]` conditions:**
- `waitFor` times out → `[FINDING] ELEMENT_NOT_FOUND: "${selector}" not present after ${timeout}ms`
- Navigation results in error page → `[FINDING] NAVIGATION_FAILED: ${url}`
- Step throws exception → `[FINDING] STEP_FAILED: ${step} — ${error}`

**Step example:** navigate -> wait for `body` -> perform one user-requested action -> verify one concrete outcome. Keep each run small; combine with `debug` or `network` when you need evidence during the flow.

**Combine with:** `debug` to catch errors mid-flow, `network` to observe API calls during the flow, `screenshot` to capture state after each step.

## scrape

**Trigger phrases:** "scrape", "extract", "collect data", "pull content", "harvest", "get all X from the page", "list all Y", "export data"

**Purpose:** Extract structured data from the live DOM. Faster and more reliable than string parsing — queries the actual rendered state including JS-rendered content.

**Domains:** `Runtime.enable`, `DOM.enable`, `Page.enable` (if navigation needed)


**Adaptive scrape recipe:**

Build the extractor around the page's real DOM. Do not cargo-cult selectors from this file. Prefer stable selectors in this order: `data-*` / test ids, semantic HTML, ARIA roles/labels, then text+link fallbacks.

For each scrape step, explicitly emit and persist reasoning:
- what hypothesis you tested
- what signal confirmed/rejected it
- what you will do next

Write this to both stdout (`[REASON]`) and session metadata (`cdp.addReasoningStep(...)`).

```js
export async function run(cdp) {
  const TARGET_URL = 'https://example.com';
  cdp.addReasoningStep?.({
    step: 'scrape-start',
    hypothesis: 'Target page contains scrapeable items in rendered DOM',
    action: `Open ${TARGET_URL} and inspect candidate selectors`,
    result: 'pending',
    nextAction: 'Navigate and extract top-level item list',
  });
  await cdp.send('Runtime.enable', {});
  await cdp.send('Page.enable', {});
  await cdp.send('Page.navigate', { url: TARGET_URL });
  await new Promise(r => setTimeout(r, 1500)); // replace with waitForNetworkIdle when needed

  const { result } = await cdp.send('Runtime.evaluate', {
    expression: `JSON.stringify({
      url: location.href,
      title: document.title,
      authWall: !!document.querySelector('input[type="password"]'),
      items: [...document.querySelectorAll('article, [data-item], li a')]
        .map(el => ({ text: el.innerText?.trim(), href: el.href || el.querySelector?.('a')?.href }))
        .filter(x => x.text)
        .slice(0, 100)
    })`,
    returnByValue: true,
  });
  const data = JSON.parse(result.value || '{}');
  cdp.upsertResourceMap?.(`page:${data.url || TARGET_URL}`, {
    type: 'page',
    url: data.url || TARGET_URL,
    title: data.title || null,
    notes: 'Scrape target page',
    tabId: cdp.targetInfo.id ?? null,
  });
  if (data.authWall) console.log('[FINDING] SCRAPE_REQUIRES_AUTH: login wall present');
  if (!data.items?.length) console.log('[FINDING] SCRAPE_EMPTY: adapt selectors or wait for rendered content');
  for (const item of data.items || []) console.log(`[SCRAPE] ${item.text} ${item.href || ''}`.trim());
  console.log(`[REASON] extracted=${data.items?.length || 0} authWall=${Boolean(data.authWall)} next=${data.authWall ? 'switch-to-user-auth' : (data.items?.length ? 'refine-or-save' : 'adjust-selectors-or-wait')}`);
  cdp.addReasoningStep?.({
    step: 'scrape-evaluate',
    hypothesis: 'Current selectors should return meaningful rows',
    action: 'Evaluated extraction result and auth-wall signal',
    result: `items=${data.items?.length || 0} authWall=${Boolean(data.authWall)}`,
    nextAction: data.authWall ? 'Run user-auth/login intent and re-run scrape' : (data.items?.length ? 'Persist dataset and optionally paginate' : 'Adjust selectors/waits and rerun'),
  });
}
```

Add consent handling, pagination, tables, file output, or authenticated reuse only when the task needs it. For browser APIs like `caches`, `navigator.storage`, or `indexedDB.databases()`, feature-detect before use and verify current docs if behavior matters.

**Multi-page loop (add only when pagination is confirmed):**
```js
while (hasNext.value) {
  await cdp.send('Runtime.evaluate', {
    expression: `document.querySelector('[class*="next"]:not([disabled]),[aria-label*="Next"]')?.click()` });
  await new Promise(r => setTimeout(r, 1500)); // wait for JS render
  // re-run extraction block and push to results array
  const { result: nextCheck } = await cdp.send('Runtime.evaluate', {
    expression: `!!(document.querySelector('[class*="next"]:not([disabled]),[aria-label*="Next"]'))`,
    returnByValue: true });
  if (!nextCheck.value) break;
}
```

**Output prefixes:** `[SCRAPE]` `[METRIC]` `[FINDING]`

Use `[REASON]` for step-by-step decision logs in long scrape loops.

**`[FINDING]` conditions:**
- Zero items extracted → `[FINDING] SCRAPE_EMPTY: selector returned 0 elements — page may require interaction or login`
- Page requires login (login form detected) → `[FINDING] SCRAPE_REQUIRES_AUTH: login wall present`
- GDPR/consent dialog intercepted navigation → `[FINDING] SCRAPE_CONSENT_WALL: consent dialog detected — handle consent, then re-run extraction`
- Data appears JS-rendered but `readyState` check fails → `[FINDING] SCRAPE_NOT_READY: content may not be loaded yet`
- Pagination detected → `[METRIC] PAGINATION_DETECTED: next page exists — use multi-page loop`

**Agent loop usage:**

```
REASON  → what data do I need? which page/selector?
SCRAPE  → set TARGET_URL, run script → read [SCRAPE] + [FINDING] lines
EXPLAIN → emit [REASON] after each step and append metadata with cdp.addReasoningStep(...)
SAVE    → results auto-saved to cdp.outputDir/scrape-results.json
MAP     → save stable resources/selectors/tab roles via cdp.upsertResourceMap(...)
PAGINATE → if [METRIC] PAGINATION_DETECTED → add multi-page loop and re-run
AUTH    → if [FINDING] SCRAPE_REQUIRES_AUTH → run login intent first, then re-run
```

Stop when: `[SCRAPE] Total: N` is non-zero and no `[FINDING] SCRAPE_EMPTY` and no more pagination.

**Combine with:** `login` (authenticate first), `emulate` (scrape as mobile), `screenshot` (capture visual proof).

## live-page

**Trigger phrases:** "open page", "open this URL", "open and monitor", "watch this page", "open browser and wait", "live check", "open and inspect", "browse with monitoring", "open it and I'll tell you what to check"

**Purpose:** Open visible Chrome, let the user drive the page, then attach for focused checks without reloads.

**Domains required:** None for the open step. Each on-demand check enables only what it needs.

**Rules:** omit `--headless`; use `--keep-tab --target-url <pattern>`; do not call `Page.navigate` in on-demand scripts.

```bash
node <skill-dir>/scripts/open-browser.mjs --url "<url>" [--port 9222]
node <skill-dir>/scripts/cdp-sandbox.mjs "$TMPDIR/cdp-<task>.mjs" \
  --target-url "<url-pattern>" --keep-tab \
  > "$TMPDIR/cdp-output-<task>.txt" 2>&1
```

Tell the user Chrome is open, then wait for their check request. Write a focused script for that request and read current page state via `Runtime.evaluate`; do not navigate.

**Minimal on-demand sketch:**

```js
export async function run(cdp) {
  const { result: urlRes } = await cdp.send('Runtime.evaluate', {
    expression: 'JSON.stringify({ url: location.href, title: document.title })',
    returnByValue: true,
  });
  console.log(`[FINDING] PAGE_STATE: ${urlRes.value}`);
}
```

| User asks | Add to script |
|---|---|
| Screenshot | `Page.captureScreenshot` |
| Current DOM / elements | `DOM.enable` → `DOM.getDocument` → `Runtime.evaluate` |
| Cookies | `Network.enable` → `Network.getAllCookies` (names only, never values) |
| localStorage / sessionStorage | `Runtime.evaluate` with `Object.keys(localStorage)` |
| Console errors so far | `Runtime.evaluate` → `window.__cdpErrors` if pre-patched, else check DOM |
| Page performance | `Runtime.evaluate` → `JSON.stringify(performance.getEntriesByType('navigation'))` |
| Network calls already made | `Runtime.evaluate` → `performance.getEntriesByType('resource')` |

> **Note:** Listeners attached after page load miss past events. Use `Runtime.evaluate` to read state already on the page (performance entries, window globals, DOM) instead of waiting for new events.

```bash
node <skill-dir>/scripts/open-browser.mjs --port 9222 --cleanup
```

**Output prefixes:** `[FINDING]` `[DOM]` `[SCREENSHOT]` `[STORAGE]` `[SECURITY]` `[METRIC]`
