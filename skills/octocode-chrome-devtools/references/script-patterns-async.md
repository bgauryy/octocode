# CDP Async And Worker Patterns

Load for waits, actionability, service workers, or worker sessions. Why: timing bugs are the most common CDP failure.

## waitForNetworkIdle
Attach Network listeners before the trigger. Track in-flight requests; resolve after a quiet window; ignore websockets/long polling when appropriate; always enforce timeout.

## waitForSelector
Poll with `Runtime.evaluate`. Require element exists, visible size, `element.matches(':disabled')` (not `element.disabled` — stays `false` under a disabled ancestor `<fieldset>`), and stable bounding box before click/fill.

Fill via the prototype's native `value` setter, not the instance setter — frameworks like React override the instance setter to track real vs programmatic changes, so plain assignment can update the DOM while the app's state never sees it. If that still doesn't stick, use `scripts/human-input.mjs`'s `buildTypingEvents` for real per-character keystrokes instead of stacking more bypass tricks.

## Service Worker Lifecycle
Enable ServiceWorker and Target auto-attach before navigation. Record registrations, versions, status, controlled clients, update errors, and console logs from worker sessions.

## Worker WebSocket
Use Target sessions. Store `{targetId, sessionId, url, role}`; pass `sessionId` as the third `cdp.send` argument.

Next: for browser artifacts/source maps load `references/script-patterns-browser.md`.
