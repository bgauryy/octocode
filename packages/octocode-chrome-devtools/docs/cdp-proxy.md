# CDP proxy

`proxy` shares one browser between several CDP clients (a second agent, a debugger UI, a script) while this
package keeps driving it. Every method, event and session id passes through unchanged.

```
proxy start  --port <browser port> [--listen-port <port>] [--log]
proxy status --port <browser port>
proxy stop   --port <browser port>
```

`start` needs a browser already answering on `--port` (run `open` first). It detaches a daemon, prints the
connection URL once and returns:

```
ws://127.0.0.1:<listenPort>/<token>
```

The URL is a secret. Anyone holding it can drive the browser, including cookies and logged-in pages. It is
printed only by `start`; `status` never shows it. The token is stored only in the 0600 state file
`<workspace>/.octocode/octocode-chrome-devtools/tmp/browser-state/cdp-proxy-<port>.json`.

## Guarantees

| Topic | Behavior |
|---|---|
| Binding | `127.0.0.1` and `::1` only, never `0.0.0.0`; `--listen-port` omitted picks a free port |
| Auth | 128-bit random token as the URL path; a wrong token, a plain HTTP request, a non-loopback `Host` or a non-loopback `Origin` header is refused with HTTP 403 before the upgrade (blocks drive-by web pages and DNS rebinding) |
| Ids | Each client's message ids are remapped on the way in and restored on the way out, so two clients that both send `id: 1` never collide |
| Events | Events with a `sessionId` go to the clients that attached or used that session; events without one reach every client |
| Clients | A client disconnecting does not close the browser connection; the proxy exits when the browser connection drops |
| Lifetime | Runs until `proxy stop`, `cleanup --port <browser port>` or the browser exits |
| `--log` | One JSON line per forwarded call (`ts`, `client`, `method`, `sessionId`, `ok`, `durationMs`) in `<workspace>/.octocode/octocode-chrome-devtools/tmp/proxy-captures/`, mode 0600. Never params or results |

`status` reports `running`, `pid`, `listenPort`, `addresses`, `clients`, `totalClients`, `messages` (client
calls forwarded), `events` (events delivered) and `logFile`. Counters refresh about every half second.

## Example

```bash
octocode-chrome-devtools open --port 9222 --headless
octocode-chrome-devtools proxy start --port 9222 --log   # prints the ws:// URL once
# any CDP client: connect to that URL, send {"id":1,"method":"Browser.getVersion"}
octocode-chrome-devtools proxy status --port 9222
octocode-chrome-devtools proxy stop --port 9222
```

## Failures

- No browser on the port: `No browser answers on port N; run open --port N first`.
- Listen port taken: `Port N is already in use; pick another --listen-port or omit it`.
- A proxy already runs for that browser port: stop it first.
