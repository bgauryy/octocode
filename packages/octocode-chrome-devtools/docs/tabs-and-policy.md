# Tab groups, attach and the origin allowlist

Everything here is an `open` option on one browser port. The group names, target ids and the
allowlist are stored in `<workspace>/.octocode/octocode-chrome-devtools/tmp/browser-state/cdp-session-meta-<port>.json`
(mode 0600, written atomically).

## Tab groups

`open --urls a,b,c [--background] [--new-window] [--isolated]` creates one tab per URL with
`Target.createTarget`, waits for each page's load, and replies:

```json
{ "status": "BROWSER_READY", "group": "g-3fa9c1", "isolatedContext": false,
  "tabs": [ { "url": "https://a", "targetId": "...", "status": "loaded" } ] }
```

Per-tab `status` is `loaded`, `timeout` (no load event within 15 s) or `error: <text>`.
The group name is generated (`g-<id>`); the reply is the only place it is announced, and `targets`
marks each member with `group` and `groupIndex`.

Select a tab with `connection.group` (and `connection.groupIndex`, default 0). It resolves to the
n-th target id of the group and behaves exactly like `--target <id>`. An unknown group, an index
out of range or a closed tab fails and lists the group's current tabs.

`--isolated` puts the whole group in one new browser context (its own cookies and storage, not
shared with the browser's default tabs). `cleanup` disposes the context first and reports
`disposedContexts`.

Chrome's visual tab groups cannot be created over CDP (verified on Chrome 158); a group here is
a named, ordered set of targets.

## Attach

`open --port N --attach` connects only to a Chrome already listening on that exact port. It never
launches a browser and never scans other ports; with nothing listening it fails and says nothing
was launched. The reply states `attached: true`, `browserVersion`, `profile` (`isolated`,
`headless isolated` or `unknown`; a real profile cannot be told from a separate one over CDP, so it
is never guessed) and `openTabCount` with `openTabs` as origin plus path (query strings can carry
tokens and are dropped).

## Origin allowlist (a mistake guard, not a boundary)

`open --port N --allow-origin https://app.example.com,https://api.example.com` stores the origins for
that port; on a browser that is already open it updates the list. A fresh launch starts with the
list of that call only.

- A plan `goto`, `after.url`, raw `Page.navigate` or `Target.createTarget`, a `--new-tab` url and
  an `open` url or url group whose origin is not listed fails **before** any connection to Chrome,
  naming the origin and the allowed list. `about:blank` is always allowed.
- While a plan runs, the page URL is checked after every step; a redirect or click that left the
  list fails that step.

This stops an agent's own mistakes (a typo, a redirect, a link followed too far). It is **not** a
security boundary: a page can still navigate itself between steps, scripts run through `script` or
`cdp` are not filtered, and a fresh launch's first `--url` is checked before launch but not after
redirects. Do not rely on it to contain an untrusted page.

## noSandbox

Chrome's sandbox stays on. `open --no-sandbox` (typed `noSandbox: true`) adds `--no-sandbox
--disable-setuid-sandbox`, as does running as root on Linux. If Chrome exits at start with "No usable
sandbox", `open` fails and says to retry with `noSandbox: true`. With the sandbox off, open only
trusted pages.
