# CDP Async And Worker Patterns

Load for waits, actionability, service workers, or worker sessions. Why: timing bugs are the most common CDP failure.

## waitForNetworkIdle
Attach Network listeners before the trigger. Track in-flight requests; resolve after a quiet window; ignore websockets/long polling when appropriate; always enforce timeout.

## waitForSelector
Poll with `Runtime.evaluate`. Require element exists, visible size, not disabled, and stable bounding box before click/fill. For framework inputs, dispatch `input` and `change`.

## Service Worker Lifecycle
Enable ServiceWorker and Target auto-attach before navigation. Record registrations, versions, status, controlled clients, update errors, and console logs from worker sessions.

## Worker WebSocket
Use Target sessions. Store `{targetId, sessionId, url, role}`; pass `sessionId` as the third `cdp.send` argument.

Next: for browser artifacts/source maps load `references/script-patterns-browser.md`.
