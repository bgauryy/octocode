# Flows: named, parametrized plans

Load when the same browser sequence repeats (sign in, fill a form, extract links, paginate, screenshot, audit). A flow is a plan with `{{param}}` placeholders; `flow run` expands it, validates the expanded plan, then executes it through the same executor as `run`.

## Commands

| Call                                               | Result                                                                                                                                                                  |
| -------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `flow list` | One compact line (inline, under the inline stdout limit): `areas` maps each area to rows `{name, description (first sentence, at most 100 characters), sensitive?, source?}`; `source` appears only on a workspace flow, every other row is built-in; unreadable saved files appear under `invalid`. Params and `expects` come from `flow show <name>` |
| `flow show <name> [--params JSON]` | The definition once: `{name, description, area, sensitive, source, params, expects, valid, missing, spliceable, steps}`. `steps` is the expanded steps array, ready to paste; `valid` is true only when no required param is missing (listed in `missing`; an unconfirmed sensitive flow lists `acknowledge`) and the expanded steps pass the real plan validator. Nothing runs; secrets show as `[secret:name]` |
| `flow run <name> --params JSON [connection flags]` | Validates params and the expanded plan before connecting, then runs it; the reply is the run's own reply plus `[FLOW] {"flow","expects"}`                               |
| `flow save <name> --plan file\|-`                  | Stores a plan (stdin or file) as `<workspace>/.octocode/octocode-chrome-devtools/flows/<name>.json`, mode 0600                                                          |

```bash
octocode-chrome-devtools /raw flow show login-form --params '{"userLabel":"User","userValue":"bob","passwordLabel":"Password","passwordValue":"pw","submitName":"Sign in","successText":"Welcome"}'
octocode-chrome-devtools /raw flow run extract-links --params '{"url":"https://example.com/"}' --port 9222
```

## Definition

`{name, description, area?, sensitive?, params: {key: {type: string|number|boolean, required?, default?, secret?, min?, max?, enum?, pattern?, description}}, plan, repeat?, expects}`. `min`/`max` bound a number, `enum` lists the allowed strings, `pattern` is a regular expression the whole string must match; all are checked before any connection.

- Placeholders `{{key}}` are replaced inside string values only; a placeholder in an object key is refused. A string value that is exactly one placeholder (`"width": "{{width}}"`) takes the param's typed value (number, boolean or string), so CDP params and numeric plan fields such as `timeoutMs` can use params. A placeholder inside a longer string is text interpolation. The expanded plan must still pass the real plan validator.
- A placeholder standing alone as a value inside a step's `params` (object key or list item) is dropped when its optional param is absent, so `Emulation.setEmulatedMedia` takes only the features given. A step whose placeholders are all absent is dropped.
- `sensitive: true` flows declare a required boolean `acknowledge`; `flow run` refuses anything but `true` before connecting, and the plan passes `acknowledge: true` on its sensitive `cdp` steps.
- Params are checked first: an unknown name, a wrong type or a missing required param is refused before any connection.
- A step that names only absent optional params is dropped (an optional `url` skips the `goto`). A step naming some absent and some present params is an error, so a `fill-form` pair needs both halves.
- `repeat: {param, offset?, steps}` appends `steps` `params[param] + offset` times; `paginate-list` uses it for `maxPages`.
- `secret: true` (strings, no default): never printed by `list`, `show`, replies or errors, and scrubbed from the files a run saved (below).

## Built-ins

| Flow                    | Required params                                                                      | Optional (default)                                          |
| ----------------------- | ------------------------------------------------------------------------------------ | ----------------------------------------------------------- |
| `login-form`            | userLabel, userValue, passwordLabel, passwordValue (secret), submitName, successText | url                                                         |
| `fill-form`             | field1Label, field1Value                                                             | url, field2..6 Label/Value (pairs), submitName, successText |
| `extract-links`         | none                                                                                 | url, selector (`a`)                                         |
| `paginate-list`         | itemSelector, nextName                                                               | url, nextRole (`link`), maxPages (5)                        |
| `full-page-screenshot`  | none                                                                                 | url                                                         |
| `console-network-audit` | url                                                                                  | none                                                        |

### CDP built-ins: plan fragments

Emulation, permission and domain-enable overrides last only for the CDP connection of the run that applies them. A flow is therefore a **plan fragment**: `flow show <name> --params ...` returns `steps` to splice into a larger `run` plan, and `flow run` applies them for that one run only (a bare `flow run emulate-device` does not change the next command). Chained steps reference earlier results as `{"$step": N, "pointer": "/root/nodeId"}`, where N counts from the first step of the plan: add the number of steps before the fragment to every `$step` when splicing, and pointers address the CDP result itself (`/root/nodeId`, not `/result/root/nodeId`). Results are saved as `cdp-<step>.json`; a step with `save: "<name>"` also writes its raw result to `saved/<name>` (the inspection flows name their key outputs this way).

Sensitive flows (marked `sensitive: true` in `flow list`) need `acknowledge: true`. Pattern lists are one pattern per flow (data cannot split a list): splice the step again for more. A flow param that is a list in the protocol (`computedStyles`, `patterns`) takes one item. Windowed flows (`ms`) end their wait by timeout on purpose; it is listed in `failedOptionalSteps` and every event is saved in `events-<id>.jsonl`.

| Area | Flow | Params (default) | Returns / effect |
| ---- | ---- | ---------------- | ---------------- |
| emulation | `emulate-device` | width, height (required); deviceScaleFactor (1), mobile (false), touch (false), userAgent | viewport, DPR, touch (`pointer: coarse`) and optional UA overridden. With `mobile: false`, `innerWidth`/`innerHeight` report the emulated size; with `mobile: true` Chrome's mobile layout can report a different `innerWidth` (read it to confirm) while `screen.width/height` report the emulated size |
| emulation | `emulate-media` | colorScheme (light, dark, no-preference), reducedMotion (reduce, no-preference); pass one or both | media features overridden. Enum string instead of a boolean: the protocol takes the text value |
| emulation | `cpu-throttle` | rate >= 1 | CPU slowed by the factor |
| emulation | `emulate-geolocation` | latitude, longitude (required); accuracy (10); acknowledge | `navigator.geolocation` position; sensitive |
| emulation | `emulate-timezone-locale` | timezone, locale; pass one or both; acknowledge true with timezone (changing the time zone needs the user permission) | time zone and locale overridden |
| permissions | `grant-permission` | permission (geolocation, notifications, clipboardReadWrite, clipboardSanitizedWrite, midi, sensors), origin (http or https); acknowledge | permission granted without a prompt; sensitive |
| permissions | `webauthn-authenticator` | protocol (ctap2), transport (internal); acknowledge | virtual authenticator; `cdp-2.json` has `authenticatorId`; sensitive |
| input | `handle-dialog` | accept (true), promptText, trigger (JavaScript that opens the dialog, run asynchronously), ms (5000) | dialog answered; the page continues. The trigger runs as an `evaluate` step |
| inspection | `css-styles` | selector | `saved/css-styles.json` computed styles, `saved/css-matched.json` matched rules |
| inspection | `accessibility-tree` | none | `saved/ax-tree.json` every AX node (role, name) |
| inspection | `page-metrics` | none | `saved/metrics.json` metrics incl. JSHeapUsedSize, `cdp-3.json` DOM counters |
| inspection | `security-info` | origin (https) | `saved/security.json` certificate chain |
| inspection | `event-listeners` | selector | `saved/event-listeners.json` listeners of the element (type, capture, script location) |
| inspection | `dom-snapshot` | computedStyle (`display`; one CSS property) | `saved/dom-snapshot.json` whole DOM with layout rectangles, paint order and the shared strings table |
| inspection | `indexeddb-dump` | origin (http or https; the page must be at it), pageSize (100); acknowledge | `saved/idb-databases.json`, `idb-database.json` (stores, indexes), `idb-data.json` (one page of the first store of the first database); sensitive |
| inspection | `cache-storage-dump` | origin (http or https; the page must be at it), pageSize (100); acknowledge | `saved/cache-names.json`, `cache-entries.json` (one page of the first cache); sensitive |
| inspection | `service-worker-state` | ms (3000) | `events-registrations.jsonl`, `events-versions.jsonl`, `events-errors.jsonl`; Chrome replays existing registrations right after enable |
| inspection | `system-info` | none | `saved/system-info.json` GPU, model and command line; a browser-level call, scope `auto` routes it |
| inspection | `storage-quota` | origin (http or https) | `saved/storage-quota.json` usage, quota and usage per storage type |
| emulation | `network-mock` | pattern (required, one URL pattern); status (200), body (base64), trigger (JavaScript that sends the request), ms (5000); acknowledge | the first request matching the pattern is answered with a made-up reply, then `Fetch` is disabled; `events-paused.jsonl` holds the paused request; sensitive |
| emulation | `block-urls` | pattern (required, one absolute URLPattern); acknowledge | matching requests are blocked for the rest of the run; sensitive |
| inspection | `layer-tree` | ms (3000) | `events-layers.jsonl` compositing layers; takes a low-quality screenshot to force the frame headless Chrome otherwise never paints |
| audits | `audits-window` | ms (3000) | `events-issues.jsonl` DevTools Issues in the window |
| audits | `animation-timeline` | ms (3000) | `events-started.jsonl`, `events-updated.jsonl`, `events-canceled.jsonl` |
| audits | `media-events` | ms (3000) | `events-events.jsonl`, `events-properties.jsonl` |

Not provided: `set-download-dir`. A plan has no variable for the run output folder, and an arbitrary download path would let a plan write outside it, so a safe flow cannot be expressed as data.

Persistence: nothing a CDP flow sets survives the run's connection. Saved evidence is the run folder only (the `cdp-<n>.json` and `events-<id>.jsonl` files above).

Fields are matched as textboxes by accessible name. `paginate-list` clicks next as an optional step: when no next control remains the click is skipped (listed in `failedOptionalSteps`) and later extracts repeat the last page. `console-network-audit` observes network (`observe.network`) and saves Runtime console and exception events plus a snapshot and a screenshot.

## Saving

`save` turns each distinct `{{name}}` into a required string param. Names containing pass, secret, token, api key or credential become `secret`. The plan must validate with every placeholder replaced by a sample string. A name equal to a built-in is refused; an existing workspace flow is never overwritten (delete its file manually). Names follow `[A-Za-z0-9][A-Za-z0-9._-]{0,63}`.

## Secrets: what is and is not covered

The executor saves the complete expanded plan (`browser-plan.json`) and may echo steps in its journal, so a typed password would persist there. `flow run` therefore rewrites every JSON/text file in the run folder after the run and scrubs stdout and stderr, replacing each secret (raw, JSON-escaped, URL-encoded) with `[REDACTED]`. Not covered: the page itself (the value was typed into it), the process argument list holding `--params` while the command runs, and a secret shorter than a few characters, which scrubs everything that contains it.
