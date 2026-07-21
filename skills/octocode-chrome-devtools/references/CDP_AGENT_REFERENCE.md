# CDP Agent Reference

Load when exact CDP domain order, session routing, or API support matters. Why: most failures are missing enables, wrong session, or stale API assumptions.

## Source Of Truth
Use official DevTools Protocol pages for unfamiliar, optional, experimental, deprecated, or failing methods: `https://chromedevtools.github.io/devtools-protocol/tot/<Domain>/`. If Chrome is running, `/json/protocol` gives exact local support.

## Enable Order
Enable domains before listeners/actions: Page, Runtime, Network, Log for debug; DOM before CSS; Fetch before request interception; ServiceWorker/Target before worker lifecycle; Performance/Tracing before measurement.

## Event Ordering
Attach listeners before navigation/action. For load evidence, open `about:blank`, add listeners, then call `Page.navigate` inside `run(cdp)`.

## Sessions
Target auto-attach creates child `sessionId`s. Route worker/iframe commands with `cdp.send(method, params, sessionId)` and track a role map.

## Safety Defaults
After `Debugger.enable`, call `Debugger.setSkipAllPauses({skip:true})`. Add a dialog guard before risky navigation. Do not output secrets.

## Common Domains
Target, Page, Runtime, DOM, CSS, Input, Network, Fetch, Emulation, Security, Debugger, Performance, Log, Browser, Storage, ServiceWorker, Accessibility.

Next: for launch flags load `references/CHROME_FLAGS.md`; for recovery load `references/RECOVERY.md`.
