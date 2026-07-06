# Browser Agent

Chrome DevTools Protocol (CDP) browser subagent for the Pi coding agent.

## Architecture

```
Main agent
  ├─ chromeDebug          ← direct single-shot CDP calls (1 scheme per call)
  ├─ spawnSubagent        ← spawn browser-agent for multi-turn sessions
  └─ browserAgent         ← generate spawn config for manual spawnAgent calls

browser-agent (subagent)
  ├─ chromeDebug          ← 28 CDP schemes, full CDP via scheme:"raw"
  ├─ web                  ← CDP docs lookup
  ├─ localGetFileContent  ← read source files, screenshots
  └─ localSearchCode      ← correlate browser errors to source

subagents/browser-agent/
  SYSTEM_PROMPT.md        ← subagent instructions (loaded at spawn)
  skills/browser-agent/   ← SKILL.md + CDP_QUICK_REF.md (loaded via --skill)
  automation/
    stealth-inject.mjs    ← 17 bot-detection evasions
    human-input.mjs       ← human-like mouse/keyboard/scroll
    detection-check.mjs   ← 14-signal stealth self-test
    README.md             ← stealth guide + detection sites
```

---

## Quick Start

### Single-shot (no subagent needed)

```
chromeDebug scheme:"debug" url:"https://example.com" port:9222 launch:true
chromeDebug scheme:"network" url:"https://example.com" port:9222
chromeDebug scheme:"screenshot" port:9222
```

### Multi-turn session

```
// 1. Spawn
spawnSubagent({
  agent: "browser-agent",
  task: "audit security of https://example.com",
  url: "https://example.com",
  port: 9222,
  launch: true
})
→ agentId: "abc123"

// 2. Wait for Phase 1 to complete
AgentMessage({action:"wait", agentId:"abc123", timeoutMs:60000})

// 3. Send Phase 2 instruction
AgentMessage({action:"send", agentId:"abc123", message:"now check cookies and storage"})
AgentMessage({action:"wait", agentId:"abc123", timeoutMs:30000})

// 4. Always kill when done
AgentMessage({action:"kill", agentId:"abc123", remove:true})
```

---

## chromeDebug — All 28 Schemes

### Core (always available)

| Scheme | What it captures | Key params |
|---|---|---|
| `debug` | Exceptions + HTTP errors + blocked + DOM state + screenshot | `url`, `durationMs` |
| `network` | All requests/responses + cookie flags | `url` (required — 0 requests on loaded tab) |
| `console` | Console messages + JS exceptions | `url`, `durationMs` |
| `dom` | Title, links, forms, error elements | `url` |
| `security` | CSP/HSTS/X-Frame + cookie flags + localStorage sensitive keys | `url` |
| `storage` | Cookies + localStorage + sessionStorage + IndexedDB + Cache + quota | `url` |
| `performance` | Core Web Vitals, JS heap, layout counts, script duration | `url` |
| `screenshot` | PNG/JPEG/PDF capture | `format`, `quality`, `fullPage` |
| `accessibility` | AX tree: unlabeled elements, missing alt, heading order | `url`, `depth` |
| `workers` | Web workers + service workers (lifecycle, scriptURL) | `url`, `durationMs` |
| `service-worker` | SW registration events, scope, status | `url`, `durationMs` |
| `websocket` | WS connections and frame monitoring | `url`, `durationMs` |
| `memory` | DOM node count, JS heap, event listener count | `url` |
| `css-coverage` | CSS rule usage after page interaction | `url`, `durationMs` |
| `js-coverage` | JS function/block coverage | `url`, `durationMs` |
| `intercept` | Request capture/mock via Fetch domain | `url`, `interceptPattern`, `mockUrl`, `mockBody` |
| `emulate` | Device viewport + UA + network throttle + geolocation | `url`, `device`, `throttle` |
| `inject` | Script injection before page load | `url`, `scriptSource`, `scriptFile`, `stealth`, `bypassCSP` |
| `scrape` | DOM data extraction with CSS/XPath | `url`, `selector`, `xpath`, `depth` |
| `monitor` | Long-running observation loop | `url`, `durationMs` |
| `consent` | GDPR/CMP audit + tracker pre-grant + dataLayer | `url`, `durationMs` |
| `supply-chain` | Third-party JS inventory + SRI checks | `url` |
| `automate` | Click, fill, wait sequences | `url`, `interact` |
| `live-page` | Attach to existing tab without reload | `expression` |
| `user-auth` | Manual auth gate — wait for login | `url`, `timeoutMs` |
| `login` | Detect auth completion + navigate | `url`, `timeoutMs` |
| `full-audit` | Runs: network + console + security + storage + accessibility + supply-chain + memory | `url`, `durationMs` |
| `raw` | **Any CDP Domain.Method** — domain auto-enabled | `method`, `params`, `scriptSource`, `scriptFile` |

### Key params

| Param | Type | Description |
|---|---|---|
| `scheme` | string | Selects the operation (required) |
| `url` | string | Navigate to this URL before running |
| `port` | integer | Chrome debug port (default 9222) |
| `launch` | boolean | Start Chrome if not running |
| `headless` | boolean | Launch headless (default false) |
| `stealth` | boolean | Inject bot-detection evasions before navigation |
| `durationMs` | integer | Observation window in ms |
| `selector` | string | CSS selector for DOM/scrape schemes |
| `expression` | string | JS expression for live-page/raw |
| `scriptSource` | string | JS to inject (avoids inline JSON escaping) |
| `scriptFile` | string | Absolute path to .mjs — loads exported *SCRIPT constant |
| `bypassCSP` | boolean | Bypass Content-Security-Policy |
| `depth` | integer | Max results (scrape) or AX tree depth |
| `xpath` | string | XPath expression for scrape |
| `method` | string | CDP `Domain.Method` for scheme:"raw" |
| `params` | object | CDP method params for scheme:"raw" |
| `interact` | object | `{click, fill, wait}` for automate |

---

## scheme:"raw" — Direct CDP Access

Domain is **auto-enabled** before the call. No manual `Domain.enable` needed.

```
chromeDebug scheme:"raw" method:"DOM.performSearch"
  params:{"query":"button","includeUserAgentShadowDOM":false}
  port:9222
```

### Common patterns

```
# Get all cookies
method:"Network.getCookies" params:{"urls":["https://example.com"]}

# Cross-frame text search
method:"DOM.performSearch" params:{"query":"login","includeUserAgentShadowDOM":false}

# Execute JS in specific iframe
method:"Page.getFrameTree" params:{}
  → get frameId
method:"Page.createIsolatedWorld" params:{"frameId":"...","worldName":"cdp","grantUniveralAccess":true}
  → get executionContextId
method:"Runtime.evaluate" params:{"expression":"document.title","contextId":N,"returnByValue":true}

# Worker network traffic (flat session model)
method:"Target.setAutoAttach" params:{"autoAttach":true,"waitForDebuggerOnStart":false,"flatten":true}
  → Target.attachedToTarget fires with sessionId
method:"Network.enable" params:{} sessionId:"<workerSessionId>"

# Heap stats (fast)
method:"Memory.getDOMCounters" params:{}

# Full AX tree
method:"Accessibility.getFullAXTree" params:{"depth":-1}
```

### Script injection (avoiding JSON escaping fragility)

**Preferred — use scriptSource:**
```
chromeDebug scheme:"inject" scriptSource:"(function(){Object.defineProperty(navigator,'webdriver',{get:()=>undefined})})()" url:"https://example.com"
```

**Or scriptFile for larger scripts:**
```
chromeDebug scheme:"inject" scriptFile:"/abs/path/to/stealth-inject.mjs" url:"https://example.com"
```

**Avoid inlining large strings in raw params** — LLM JSON escaping is unreliable for >500 char strings.

---

## Stealth Mode

Inject bot-detection evasions before navigation:

```
chromeDebug scheme:"inject" stealth:true url:"https://example.com" port:9222
```

### 17 evasions patched

| Signal | Patch |
|---|---|
| `navigator.webdriver` | Delete → `undefined` |
| `window.chrome` | Add runtime + csi + loadTimes |
| `chrome.app` | Add InstallState/RunningState |
| `navigator.plugins` | Spoof 3 real plugins |
| `navigator.languages` | `['en-US','en']` |
| `navigator.vendor` | `'Google Inc.'` |
| `navigator.hardwareConcurrency` | 4 if below threshold |
| Permissions | `'default'` not `'denied'` |
| User-Agent | Strip HeadlessChrome |
| WebGL vendor/renderer | Intel instead of SwiftShader |
| Canvas fingerprint | 1-bit LSB noise |
| iframe contentWindow | Patch webdriver inside iframes |
| screen dimensions | 1920×1080 |
| outerWidth/outerHeight | outer ≥ inner |
| media.canPlayType | H.264/AAC codec support |
| `chrome.app` | Realistic object |
| `navigator.hardwareConcurrency` | ≥4 |

### Detection check

```
chromeDebug scheme:"raw" method:"Runtime.evaluate"
  params:{"expression":"<content of automation/detection-check.mjs DETECTION_CHECK_SCRIPT>","returnByValue":true,"awaitPromise":true}
```

Returns: `{score:N, total:14, verdict:"CLEAN"|"MOSTLY_CLEAN"|"DETECTED"}`

---

## Multi-Turn Protocol

### Output prefixes (mandatory)

| Prefix | When |
|---|---|
| `[STATUS]` | Start of every operation |
| `[FINDING]` | Issue or discovery |
| `[ACTION]` | Recommended next step |
| `[METRIC]` | Count, size, duration, % |
| `[SCREENSHOT]` | Absolute path to PNG |
| `[BLOCKED]` | Cannot proceed — state what's needed |
| `[DONE]` | Phase complete — stop and wait |

### Multi-turn discipline

**One phase per turn:**
1. Complete the task given for this turn
2. Emit `[DONE] summary`
3. **Stop — do not proceed to next phase**
4. Main agent reads [DONE] and sends next instruction

### [BLOCKED] triggers (stop immediately)

- Chrome not running and `launch:true` not set
- Page requires authentication
- Task is ambiguous
- Action would modify real user data

### Communication patterns

```
// Basic
agentId = spawnSubagent(...)
AgentMessage({action:"wait", agentId, timeoutMs:60000})
AgentMessage({action:"kill", agentId, remove:true})

// Async polling (long tasks > 30s)
while (status !== "idle") {
  AgentMessage({action:"status", agentId})
  sleep 10s
}

// Parallel browsers
a = spawnSubagent({..., port:9222})
b = spawnSubagent({..., port:9223})
AgentMessage({action:"wait", agentId:a, timeoutMs:90000})
AgentMessage({action:"wait", agentId:b, timeoutMs:90000})

// Steer (interrupt wrong direction)
AgentMessage({action:"steer", agentId, message:"focus on cookies only"})

// Always kill after last [DONE]
AgentMessage({action:"kill", agentId, remove:true})
```

---

## Chrome Launch

Each port gets its own isolated profile: `~/.octocode/chrome-debug/profile-<port>/`

### Launch flags (automation-optimized)

```
--remote-debugging-port=N
--user-data-dir=~/.octocode/chrome-debug/profile-N  ← port-specific (parallel safety)
--no-first-run --no-default-browser-check
--disable-background-networking --disable-extensions
--disable-popup-blocking --disable-translate --disable-sync
--password-store=basic --safebrowsing-disable-auto-update
--use-mock-keychain (macOS)
--disable-features=TranslateUI,MediaRouter,OptimizationHints
--headless=new --disable-gpu --disable-dev-shm-usage (headless)
--hide-scrollbars --mute-audio (headless)
```

### Guards (automatic on every navigation)

- `Page.handleJavaScriptDialog` → auto-dismiss alert/confirm/prompt (prevents CDP hang)
- `Debugger.setSkipAllPauses({skip:true})` → skip `debugger;` statements (prevents eval freeze)

---

## CDP Event Log (terminal visibility)

```bash
# Enable
OCTOCODE_CDP_DEBUG=1 pi ...

# Tail raw CDP traffic
tail -f ~/.octocode/chrome-debug/port-9222/cdp-events.jsonl

# Pretty-print
tail -f ~/.octocode/chrome-debug/port-9222/cdp-events.jsonl | \
  python3 -c "import sys,json; [print(json.dumps(json.loads(l))) for l in sys.stdin]"
```

---

## Automation Files

```
subagents/browser-agent/automation/
  stealth-inject.mjs    — 17 evasions, inject via scriptFile or scriptSource
  human-input.mjs       — Bezier mouse, natural typing, scroll
  detection-check.mjs   — 14-signal stealth test script
  README.md             — detection sites, signal table, usage guide
  workflows/            — example multi-turn workflow scripts
```

---

## Token Efficiency

Live measurements (Chrome 150, headless):

| Task | Schemes | Chars | Tokens |
|---|---|---|---|
| example.com baseline | 5 | 954 | ~239 |
| arxiv.org full audit | 8 | 3,160 | ~790 |
| x.com security | 9 | 5,500 | ~1,375 |
| octocode.ai full audit | 16 | ~11,400 | ~2,850 |

vs playwright-mcp: ~114,000 chars (~28,500 tokens) for equivalent analysis — **~10× more efficient**.

---

## Pi Improvement Proposals

See `.octocode/plans/pi-improvements/PI_IMPROVEMENTS.md` for open proposals:

1. **Extension hot-reload** — `spawnSubagent` tool requires session restart after build
2. **Agent idle callback** — no `onAgentIdle` hook for auto-cleanup
3. **AgentMessage cross-process** — sub-orchestrator agent IDs not visible to parent
4. **`--skill` in SpawnAgentParams** — ✅ implemented (`skills?: string[]`)
