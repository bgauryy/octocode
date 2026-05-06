#!/usr/bin/env node
// Run a generated `run(cdp)` script against a Chrome CDP target.

import { resolve, join } from 'path';
import { existsSync, mkdirSync } from 'fs';
import { pathToFileURL } from 'url';
import { tmpdir } from 'os';

const argv      = process.argv.slice(2);
const scriptArg = argv.find(a => !a.startsWith('--') && (a.endsWith('.mjs') || a.endsWith('.js')));
const getArg    = (flag, def) => { const i = argv.indexOf(flag); return i !== -1 && argv[i + 1] ? argv[i + 1] : def; };
const hasFlag   = (flag) => argv.includes(flag);

const PORT        = getArg('--port', '9222');
const NEW_TAB     = getArg('--new-tab', '');
const TARGET_ID   = getArg('--target', '');
const TARGET_URL  = getArg('--target-url', '');   // substring match on target URL
const TARGET_TYPE = getArg('--target-type', '');  // page | iframe | service_worker | worker
const TIMEOUT     = parseInt(getArg('--timeout', '60000'), 10);
const KEEP_TAB    = hasFlag('--keep-tab');
const LIST_TARGETS = hasFlag('--list-targets');

if (!scriptArg && !LIST_TARGETS) {
  console.error('[CDP_RUNNER] Usage: node cdp-runner.mjs <script.mjs> [--port 9222] [--new-tab <url>] [--target <id>] [--target-url <pattern>] [--target-type <type>] [--list-targets] [--keep-tab]');
  process.exit(1);
}

const [nodeMajor] = process.versions.node.split('.').map(Number);
if (nodeMajor < 22) {
  console.error(`[CDP_RUNNER] Node.js 22+ required (you have ${process.versions.node}). Native WebSocket is unavailable.`);
  process.exit(1);
}
const WS = globalThis.WebSocket;

async function cdpHttp(path, method = 'GET') {
  const res = await fetch(`http://localhost:${PORT}${path}`, { method, signal: AbortSignal.timeout(5000) });
  if (!res.ok) throw new Error(`CDP HTTP ${res.status} for ${path}`);
  return res.json();
}

async function getVersion()       { return cdpHttp('/json/version'); }
async function getTargets()       { return cdpHttp('/json'); }
async function openTab(url)       { return cdpHttp(`/json/new?${encodeURIComponent(url)}`, 'PUT'); }
async function activateTarget(id) { return cdpHttp(`/json/activate/${id}`); }
async function closeTab(id)       {
  try {
    const res = await fetch(`http://localhost:${PORT}/json/close/${id}`, { signal: AbortSignal.timeout(3000) });
    return res.ok;
  } catch { return false; }
}

function createSession(wsUrl, targetInfo) {
  return new Promise((resolveSession, rejectSession) => {
    const ws = new WS(wsUrl);
    let msgId = 1;
    const pending  = new Map();  // id → { resolve, reject, timer }
    const handlers = new Map();  // eventName → Set<handler>
    let closed = false;

    function drainPending(reason) {
      if (pending.size === 0) return;
      const err = new Error(reason);
      pending.forEach(({ rej, timer }) => { clearTimeout(timer); rej(err); });
      pending.clear();
    }

    ws.onopen = () => {
      const session = {
        targetInfo,

        send(method, params = {}) {
          if (closed) return Promise.reject(new Error('Session already closed'));
          return new Promise((res, rej) => {
            const id = msgId++;
            const timer = setTimeout(() => {
              pending.delete(id);
              rej(new Error(`CDP timeout (${TIMEOUT}ms) for: ${method}`));
            }, TIMEOUT);
            pending.set(id, { res, rej, timer });
            ws.send(JSON.stringify({ id, method, params }));
          });
        },

        on(event, handler) {
          if (!handlers.has(event)) handlers.set(event, new Set());
          handlers.get(event).add(handler);
        },

        off(event, handler) {
          handlers.get(event)?.delete(handler);
        },

        log(...args) {
          console.log('[BROWSER]', ...args);
        },

        outputDir: '',  // set after session creation — use cdp.outputDir to save files

        close() {
          if (closed) return;
          closed = true;
          drainPending('Session closed');
          handlers.clear();
          try { ws.close(); } catch {}
        },
      };

      resolveSession(session);
    };

    ws.onmessage = (evt) => {
      let msg;
      try { msg = JSON.parse(typeof evt === 'string' ? evt : evt.data); } catch { return; }

      if (msg.id !== undefined && pending.has(msg.id)) {
        const { res, rej, timer } = pending.get(msg.id);
        pending.delete(msg.id);
        clearTimeout(timer);
        if (msg.error) rej(new Error(`CDP error [${msg.error.code}]: ${msg.error.message}`));
        else res(msg.result ?? {});
      } else if (msg.method) {
        handlers.get(msg.method)?.forEach(h => {
          try { h(msg.params ?? {}); } catch (e) { console.error('[CDP_RUNNER] Handler error:', e.message); }
        });
        handlers.get('*')?.forEach(h => {
          try { h(msg.method, msg.params ?? {}); } catch {}
        });
      }
    };

    ws.onerror = (e) => {
      const msg = e?.message ?? String(e);
      drainPending(`WebSocket error: ${msg}`);
      if (!closed) rejectSession(new Error(`WebSocket error: ${msg}`));
    };

    ws.onclose = () => {
      drainPending('WebSocket closed unexpectedly');
    };
  });
}

let _cleanup = null;
function registerCleanup(fn) { _cleanup = fn; }

async function shutdown(signal) {
  console.error(`[CDP_RUNNER] ${signal} received — cleaning up...`);
  if (_cleanup) {
    try { await _cleanup(); } catch {}
  }
  process.exit(0);
}

process.on('SIGINT',  () => shutdown('SIGINT'));
process.on('SIGTERM', () => shutdown('SIGTERM'));

async function main() {
  // 1. Verify browser is running
  let version;
  try {
    version = await getVersion();
  } catch {
    console.error(`[CDP_RUNNER] Chrome not responding on port ${PORT}. Run open-browser.mjs first.`);
    process.exit(1);
  }
  console.error(`[CDP_RUNNER] Chrome: ${version.Browser}`);

  // 2. List targets (--list-targets discovery mode)
  if (LIST_TARGETS) {
    const targets = await getTargets();
    console.log(JSON.stringify(targets.map(t => ({
      id: t.id, type: t.type, url: t.url, title: t.title,
    })), null, 2));
    process.exit(0);
  }

  // 3. Resolve target
  let targetWsUrl, targetInfo, openedTabId;

  if (NEW_TAB) {
    const tab  = await openTab(NEW_TAB);
    openedTabId = tab.id;
    targetWsUrl = tab.webSocketDebuggerUrl;
    targetInfo  = { id: tab.id, url: tab.url, title: tab.title, type: tab.type };
    console.error(`[CDP_RUNNER] Opened new tab (${tab.id}) → ${NEW_TAB}`);
    await new Promise(r => setTimeout(r, 800));

  } else if (TARGET_ID) {
    const targets = await getTargets();
    const t = targets.find(x => x.id === TARGET_ID);
    if (!t) { console.error(`[CDP_RUNNER] Target ${TARGET_ID} not found`); process.exit(1); }
    targetWsUrl = t.webSocketDebuggerUrl;
    targetInfo  = t;
    await activateTarget(TARGET_ID).catch(() => {});

  } else if (TARGET_URL) {
    // Match by URL substring — finds iframes, service workers, workers by URL pattern
    const targets = await getTargets();
    const pool    = TARGET_TYPE ? targets.filter(t => t.type === TARGET_TYPE) : targets;
    const t       = pool.find(x => x.url && x.url.includes(TARGET_URL));
    if (!t) {
      const available = targets.map(x => `  [${x.type}] ${x.url}`).join('\n');
      console.error(`[CDP_RUNNER] No target URL matching "${TARGET_URL}". Available targets:\n${available}`);
      process.exit(1);
    }
    targetWsUrl = t.webSocketDebuggerUrl;
    targetInfo  = t;
    console.error(`[CDP_RUNNER] Matched target [${t.type}]: ${t.url}`);

  } else if (TARGET_TYPE) {
    // Match by type only (e.g. service_worker, iframe)
    const targets = await getTargets();
    const t       = targets.find(x => x.type === TARGET_TYPE);
    if (!t) {
      const available = [...new Set(targets.map(x => x.type))].join(', ');
      console.error(`[CDP_RUNNER] No target of type "${TARGET_TYPE}". Available types: ${available}`);
      process.exit(1);
    }
    targetWsUrl = t.webSocketDebuggerUrl;
    targetInfo  = t;
    console.error(`[CDP_RUNNER] Matched target [${t.type}]: ${t.url}`);

  } else {
    // Default: first open page
    const targets = await getTargets();
    const pages   = targets.filter(t => t.type === 'page');
    if (pages.length === 0) {
      console.error('[CDP_RUNNER] No page targets. Open a tab in Chrome first, or use --new-tab <url>');
      process.exit(1);
    }
    const t = pages[0];
    targetWsUrl = t.webSocketDebuggerUrl;
    targetInfo  = t;
    console.error(`[CDP_RUNNER] Using tab: ${t.url}`);
  }

  if (!targetWsUrl) {
    console.error('[CDP_RUNNER] Could not get WebSocket URL for target');
    process.exit(1);
  }

  // 4. Connect
  const cdp = await createSession(targetWsUrl, targetInfo);

  // Resolve output directory: prefer CDP_OUTPUT_DIR injected by cdp-sandbox.mjs,
  // otherwise create a fresh timestamped dir (direct runner invocation).
  const outputDir = process.env.CDP_OUTPUT_DIR ?? (() => {
    const ts  = new Date().toISOString().slice(0, 19).replace(/[:T]/g, '-');
    const dir = join(tmpdir(), '.octocode-chrome-devtools', ts);
    mkdirSync(dir, { recursive: true });
    return dir;
  })();
  cdp.outputDir = outputDir;
  console.error(`[CDP_RUNNER] Output dir: ${outputDir}`);
  console.error(`[CDP_RUNNER] Connected — running ${scriptArg}`);

  // Restrict network access to localhost only for code running inside the user script.
  // The CDP WebSocket and HTTP endpoints above are already established (localhost).
  // This patch prevents a generated script from using Node's fetch/WebSocket to reach
  // external hosts directly — compensating for Node's PM lacking --allow-net scoping.
  const _origFetch = globalThis.fetch;
  const _OrigWS    = globalThis.WebSocket;
  function isLocalhost(url) {
    try {
      const h = new URL(String(url)).hostname;
      return h === 'localhost' || h === '127.0.0.1' || h === '::1';
    } catch { return false; }
  }
  globalThis.fetch = function restrictedFetch(input, init) {
    const url = typeof input === 'string' ? input
      : input instanceof URL ? input.href
      : input?.url ?? '';
    if (!isLocalhost(url)) {
      throw new Error(`[SANDBOX] fetch blocked: only localhost allowed (attempted: ${url})`);
    }
    return _origFetch(input, init);
  };
  globalThis.WebSocket = class RestrictedWebSocket extends _OrigWS {
    constructor(url, ...args) {
      if (!isLocalhost(url)) {
        throw new Error(`[SANDBOX] WebSocket blocked: only localhost allowed (attempted: ${url})`);
      }
      super(url, ...args);
    }
  };

  // Register cleanup: close WebSocket + close tab (if we opened it)
  registerCleanup(async () => {
    cdp.close();
    if (openedTabId && !KEEP_TAB) {
      const closed = await closeTab(openedTabId);
      console.error(`[CDP_RUNNER] Tab ${openedTabId} ${closed ? 'closed' : 'already gone'}`);
    }
  });

  // 5. Load script
  const scriptPath = resolve(process.cwd(), scriptArg);
  if (!existsSync(scriptPath)) {
    console.error(`[CDP_RUNNER] Script not found: ${scriptPath}`);
    await _cleanup?.();
    process.exit(1);
  }

  let mod;
  try {
    mod = await import(pathToFileURL(scriptPath).href);
  } catch (e) {
    console.error(`[CDP_RUNNER] Failed to load script: ${e.message}`);
    await _cleanup?.();
    process.exit(1);
  }

  if (typeof mod.run !== 'function') {
    console.error('[CDP_RUNNER] Script must export: export async function run(cdp) { ... }');
    await _cleanup?.();
    process.exit(1);
  }

  // 6. Run script — always cleanup in finally
  let exitCode = 0;
  try {
    await mod.run(cdp);
    console.error('[CDP_RUNNER] Script completed successfully');
  } catch (e) {
    const isCdpError = /CDP error \[|CDP timeout/.test(e.message);
    if (isCdpError) {
      // Exit code 2 = fixable CDP error — agent should correct the script and retry once
      // Extract method from timeout: "CDP timeout for: Domain.method"
      // or from not-found: "'Domain.method' wasn't found"
      const methodMatch = e.message.match(/for:\s*(\S+)/) ?? e.message.match(/'([A-Z][a-zA-Z]+\.[a-zA-Z]+)'/);
      const method = methodMatch ? methodMatch[1] : 'unknown';
      console.log(`[CDP_RETRY_NEEDED] method=${method} error="${e.message}"`);
      console.log(`[CDP_RETRY_NEEDED] Fix: ensure the domain for "${method}" is enabled before calling it, check parameter names, and re-run.`);
      exitCode = 2;
    } else {
      console.error(`[CDP_RUNNER] Script error: ${e.message}`);
      if (e.stack) console.error(e.stack);
      exitCode = 1;
    }
  } finally {
    await _cleanup?.();
    _cleanup = null;
  }

  process.exit(exitCode);
}

main().catch(async e => {
  console.error('[CDP_RUNNER_FATAL]', e.message);
  if (_cleanup) { try { await _cleanup(); } catch {} }
  process.exit(1);
});
