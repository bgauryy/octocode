# Tab groups, attach and the origin allowlist

Tab groups belong to the `group` tool; attach and the allowlist are `open` options on one browser port. The group names, target ids, private contexts and the
allowlist are stored in `<workspace>/.octocode/octocode-chrome-devtools/tmp/browser-state/cdp-session-meta-<port>.json`
(mode 0600, written atomically).

## Tab groups

`group` `{action, name?, urls?, private?, background?, newWindow?, tab?, port}` manages named tab groups on one browser port. A **private** group (the default) is one isolated browser context (`Target.createBrowserContext`): its tabs share cookies and storage with each other and with nothing else, like incognito. `private: false` puts the tabs in the default context. Chrome's visual tab groups cannot be created over CDP (verified on Chrome 158); a group here is a named, ordered set of targets.

| Action | Fields | Effect |
|---|---|---|
| `open` | `name?` (letters, digits, `.` `_` `-`, at most 64; generated `g-xxxxxx` when omitted), `urls?`, `private?`, `background?`, `newWindow?` | Creates the group, its context when private, and one tab per URL (`http(s)` or `about:blank`). Refuses a name that exists |
| `add` | `name`, `urls` (non-empty), `background?` | Appends tabs in the group's own context. An unknown name lists the known groups |
| `list` | none | Every group: `name`, `private`, `tabCount`, tabs `{targetId, url, title?, alive}` |
| `close` | `name`, `tab?` | With `tab` (a targetId of the group): closes that tab, the group stays. Without: closes every tab, disposes the context, frees the name |

Examples (replies are JSON):

```
group open  {name:"shop", urls:["https://a.example/","https://a.example/cart"]}
  -> {group:"shop", private:true, tabs:[{url, targetId, status:"loaded"|"timeout"|"error: ..."}, ...]}
group add   {name:"shop", urls:["https://a.example/pay"]}   -> {group, private, tabs:[...]}
group list  {}
  -> {groups:[{name:"shop", private:true, tabCount:3, tabs:[{targetId, url:"https://a.example/cart", title, alive:true}, ...]}]}
group close {name:"shop", tab:"<targetId>"}                  -> {group, closedTab}
group close {name:"shop"}                                    -> {group, closed:true, closedTabs:2}
```

- `list` reconciles with the browser (`Target.getTargets`): a tab the user closed shows `alive:false` once and is then removed from the stored group, so later indexes shift. Urls are origin plus path only.
- The session's allowlist applies: every URL of `open`/`add` outside it is refused before any context or tab is created, and a tab that redirects out reports `status: "error: redirected ..."`.
- Select a tab in other tools with `connection.group` (and `connection.groupIndex`, default 0); it resolves like `--target <id>`. An unknown group, an index out of range or a closed tab fails and lists the group's tabs. `targets` rows of group tabs carry `group`, `groupIndex` and `private`.
- Tabs outside a group are never touched. `cleanup` disposes every remaining private context. State is stored in the session meta file (below), mode 0600, written atomically.

The raw argv is `group <action> [--name N] [--urls a,b] [--no-private] [--background] [--new-window] [--tab ID] --port N`.

## Attach

`open --port N --attach --acknowledge` (typed `attach: true` and `acknowledge: true`) connects only to a Chrome already listening on that exact port. It never
launches a browser and never scans other ports; with nothing listening it fails and says nothing
was launched. The reply states `attached: true`, `browserVersion`, `profile` (`isolated`,
`headless isolated` or `unknown`; a real profile cannot be told from a separate one over CDP, so it
is never guessed) and `openTabCount` with `openTabs` as origin plus path (query strings can carry
tokens and are dropped).

Attaching exposes the browser's logged-in pages and cookies, so `acknowledge: true` is required with
`attach`: set it only after the user explicitly agrees. The contract refuses `attach` without it, and the
engine refuses `--attach` without `--acknowledge` too, so `/raw` cannot skip the consent. A foreign
listener that is opened without `attach` is refused with the same instruction.

## URL schemes that need consent

Independent of the allowlist, these schemes are refused for plan `goto`, `--new-tab`, `open --url`, `group` urls and `Page.navigate` / `Target.createTarget` params: `file:` (reads local files), `chrome:`, `chrome-untrusted:`, `devtools:` (browser internals), `chrome-extension:`, `view-source:`, `javascript:`, and `about:` other than `about:blank`. Only a `cdp` call with `acknowledge: true` may navigate to them; ask the user first. `data:` and `about:blank` stay allowed. The table and the reasons are in [cdp-protocol.md](cdp-protocol.md#safety-policy).

## Origin allowlist (a mistake guard, not a boundary)

`open --port N --allow-origin https://app.example.com,https://api.example.com` stores the origins for
that port; on a browser that is already open it updates the list. A fresh launch starts with the
list of that call only.

- A plan `goto`, `after.url`, raw `Page.navigate` or `Target.createTarget`, a `--new-tab` url and
  an `open` url or a `group` url whose origin is not listed fails **before** any connection to Chrome,
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

## Emulation that persists

`open --port N --emulation '<json>'` (typed `emulation`) stores a device and environment emulation for that port. Keys: `width`, `height` (positive integers), `deviceScaleFactor` (> 0), `mobile`, `userAgent`, `colorScheme` (`light`/`dark`/`no-preference`), `reducedMotion`, `timezone`, `locale`, `geolocation {latitude -90..90, longitude -180..180, accuracy >= 0}`, `cpuThrottle` (>= 1), `touch`, or `{reset:true}` alone.

- Stored under `emulation` in the port's session meta file (mode 0600, atomic). Each non-reset `open --emulation` replaces it; `{reset:true}` deletes it; an `open` without `--emulation` on a tracked reuse or an attach keeps it; a fresh launch and `cleanup` clear it. Ports are independent.
- The reply carries `emulation: {applied: [keys], persistent: true}` (geolocation shows as `geolocation: set`, never the coordinates) or `emulation: {reset: true}`.
- `geolocation` spoofs the browser location: it needs `--acknowledge` (`acknowledge: true`), refused otherwise with nothing stored.
- Chrome drops these overrides when the CDP connection closes, so `cdp-runner` re-sends the stored object right after it attaches and before any script, plan or check runs (every runner-based call; new tabs too). Calls: `Emulation.setDeviceMetricsOverride`, `setUserAgentOverride` (also carries `acceptLanguage` so `navigator.language` follows `locale`), `setEmulatedMedia`, `setTimezoneOverride`, `setLocaleOverride`, `setGeolocationOverride`, `setCPUThrottlingRate`, `setTouchEmulationEnabled`. A failed call is a `[WARNING] <method> failed` line on stderr and never aborts the run; a browser-level target (`--browser`) skips them with a note. No stored emulation costs no CDP call.
- Permission grants (`Browser.grantPermissions`) also last only for their own connection: grant and read geolocation inside one run.
