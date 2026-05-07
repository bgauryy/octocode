#!/usr/bin/env node
// Run a generated `run(cdp)` script against a Chrome CDP target.

import { resolve, join } from 'path';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'fs';
import { pathToFileURL } from 'url';
import { tmpdir } from 'os';

const argv      = process.argv.slice(2);
const scriptArg = argv.find(a => !a.startsWith('--') && (a.endsWith('.mjs') || a.endsWith('.js')));
const getArg    = (flag, def) => { const i = argv.indexOf(flag); return i !== -1 && argv[i + 1] ? argv[i + 1] : def; };
const hasFlag   = (flag) => argv.includes(flag);

const PORT        = getArg('--port', '9222');
const NEW_TAB     = getArg('--new-tab', '');
const TARGET_ID   = getArg('--target', '');
const TARGET_URL  = getArg('--target-url', '');
const TARGET_TYPE = getArg('--target-type', '');
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

function readJson(filePath, fallback = null) {
  try { return JSON.parse(readFileSync(filePath, 'utf8')); } catch { return fallback; }
}

function writeJson(filePath, value) {
  writeFileSync(filePath, `${JSON.stringify(value, null, 2)}\n`, { mode: 0o600 });
}

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
    const pending  = new Map();
    const handlers = new Map();
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

        send(method, params = {}, sessionId = undefined) {
          if (closed) return Promise.reject(new Error('Session already closed'));
          return new Promise((res, rej) => {
            const id = msgId++;
            const timer = setTimeout(() => {
              pending.delete(id);
              rej(new Error(`CDP timeout (${TIMEOUT}ms) for: ${method}`));
            }, TIMEOUT);
            pending.set(id, { res, rej, timer });
            const payload = { id, method, params };
            if (sessionId) payload.sessionId = sessionId;
            ws.send(JSON.stringify(payload));
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

        outputDir: '',

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
        const meta = msg.sessionId ? { sessionId: msg.sessionId } : {};
        handlers.get(msg.method)?.forEach(h => {
          try { h(msg.params ?? {}, meta); } catch (e) { console.error('[CDP_RUNNER] Handler error:', e.message); }
        });
        handlers.get('*')?.forEach(h => {
          try { h(msg.method, msg.params ?? {}, meta); } catch {}
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
  console.error(`[CDP_RUNNER] ${signal} received - cleaning up...`);
  if (_cleanup) {
    try { await _cleanup(); } catch {}
  }
  process.exit(0);
}

process.on('SIGINT',  () => shutdown('SIGINT'));
process.on('SIGTERM', () => shutdown('SIGTERM'));

async function main() {
  let version;
  try {
    version = await getVersion();
  } catch {
    console.error(`[CDP_RUNNER] Chrome not responding on port ${PORT}. Run open-browser.mjs first.`);
    process.exit(1);
  }
  console.error(`[CDP_RUNNER] Chrome: ${version.Browser}`);
  const sessionMetaDir = process.env.CDP_SESSION_META_DIR ?? (() => {
    const dir = join(tmpdir(), '.octocode-chrome-devtools', 'session-meta', `port-${PORT}`);
    mkdirSync(dir, { recursive: true, mode: 0o700 });
    return dir;
  })();
  mkdirSync(sessionMetaDir, { recursive: true, mode: 0o700 });
  const sessionMetaFile = join(sessionMetaDir, 'session-metadata.json');
  const targetSnapshotFile = join(sessionMetaDir, 'targets-latest.json');

  if (LIST_TARGETS) {
    const nowIso = new Date().toISOString();
    const targets = await getTargets();
    writeJson(targetSnapshotFile, {
      capturedAt: nowIso,
      port: PORT,
      targets: targets.map(t => ({
        id: t.id ?? null,
        type: t.type ?? null,
        url: t.url ?? null,
        title: t.title ?? null,
      })),
    });
    const existingMeta = readJson(sessionMetaFile, {}) ?? {};
    writeJson(sessionMetaFile, {
      ...existingMeta,
      port: PORT,
      browser: version.Browser,
      lastListedTargetsAt: nowIso,
      updatedAt: nowIso,
    });
    console.log(JSON.stringify(targets.map(t => ({
      id: t.id, type: t.type, url: t.url, title: t.title,
    })), null, 2));
    process.exit(0);
  }

  let targetWsUrl, targetInfo, openedTabId;

  if (NEW_TAB) {
    const tab  = await openTab(NEW_TAB);
    openedTabId = tab.id;
    targetWsUrl = tab.webSocketDebuggerUrl;
    targetInfo  = { id: tab.id, url: tab.url, title: tab.title, type: tab.type };
    console.error(`[CDP_RUNNER] Opened new tab (${tab.id}) -> ${NEW_TAB}`);
    await new Promise(r => setTimeout(r, 800));

  } else if (TARGET_ID) {
    const targets = await getTargets();
    const t = targets.find(x => x.id === TARGET_ID);
    if (!t) { console.error(`[CDP_RUNNER] Target ${TARGET_ID} not found`); process.exit(1); }
    targetWsUrl = t.webSocketDebuggerUrl;
    targetInfo  = t;
    await activateTarget(TARGET_ID).catch(() => {});

  } else if (TARGET_URL) {
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

  const cdp = await createSession(targetWsUrl, targetInfo);

  const outputDir = process.env.CDP_OUTPUT_DIR ?? (() => {
    const ts  = new Date().toISOString().slice(0, 19).replace(/[:T]/g, '-');
    const dir = join(tmpdir(), '.octocode-chrome-devtools', ts);
    mkdirSync(dir, { recursive: true });
    return dir;
  })();
  const runLogFile = join(sessionMetaDir, 'run-history.json');
  const existingMeta = readJson(sessionMetaFile, {}) ?? {};
  const nowIso = new Date().toISOString();
  const baseMeta = {
    ...existingMeta,
    port: PORT,
    browser: version.Browser,
    lastConnectedAt: nowIso,
    outputDir,
    lastScript: scriptArg,
    currentTarget: {
      id: targetInfo.id ?? null,
      type: targetInfo.type ?? null,
      url: targetInfo.url ?? null,
      title: targetInfo.title ?? null,
      via: NEW_TAB ? 'new-tab' : TARGET_ID ? 'target' : TARGET_URL ? 'target-url' : TARGET_TYPE ? 'target-type' : 'first-page',
    },
    lastSelection: {
      newTab: NEW_TAB || null,
      targetId: TARGET_ID || null,
      targetUrl: TARGET_URL || null,
      targetType: TARGET_TYPE || null,
      keepTab: KEEP_TAB,
    },
    updatedAt: nowIso,
  };
  writeJson(sessionMetaFile, baseMeta);

  const currentTargets = await getTargets().catch(() => []);
  writeJson(targetSnapshotFile, {
    capturedAt: nowIso,
    port: PORT,
    targets: currentTargets.map(t => ({
      id: t.id ?? null,
      type: t.type ?? null,
      url: t.url ?? null,
      title: t.title ?? null,
      attached: t.id === targetInfo.id,
    })),
  });

  const runHistory = readJson(runLogFile, { runs: [] }) ?? { runs: [] };
  if (!Array.isArray(runHistory.runs)) runHistory.runs = [];
  const runId = `${Date.now()}-${Math.random().toString(36).slice(2, 10)}`;
  runHistory.runs.push({
    id: runId,
    startedAt: nowIso,
    script: scriptArg,
    outputDir,
    target: baseMeta.currentTarget,
    status: 'running',
  });
  if (runHistory.runs.length > 100) runHistory.runs = runHistory.runs.slice(-100);
  writeJson(runLogFile, runHistory);
  const finalizeRun = (status, extra = {}) => {
    const current = readJson(runLogFile, { runs: [] }) ?? { runs: [] };
    if (!Array.isArray(current.runs)) current.runs = [];
    const idx = current.runs.findIndex(r => r.id === runId);
    if (idx !== -1) {
      current.runs[idx] = {
        ...current.runs[idx],
        status,
        finishedAt: new Date().toISOString(),
        ...extra,
      };
      writeJson(runLogFile, current);
    }
  };

  cdp.outputDir = outputDir;
  cdp.sessionMetaDir = sessionMetaDir;
  cdp.sessionMetaFile = sessionMetaFile;
  cdp.targetSnapshotFile = targetSnapshotFile;
  cdp.resourcesFile = join(sessionMetaDir, 'resource-map.json');
  cdp.reasoningFile = join(sessionMetaDir, 'reasoning-log.json');
  cdp.addReasoningStep = (step) => {
    const payload = readJson(cdp.reasoningFile, { steps: [] }) ?? { steps: [] };
    if (!Array.isArray(payload.steps)) payload.steps = [];
    payload.steps.push({
      at: new Date().toISOString(),
      ...step,
    });
    if (payload.steps.length > 300) payload.steps = payload.steps.slice(-300);
    writeJson(cdp.reasoningFile, payload);
    return payload.steps.length;
  };
  cdp.upsertResourceMap = (resourceKey, details) => {
    const payload = readJson(cdp.resourcesFile, { updatedAt: null, resources: {} }) ?? { updatedAt: null, resources: {} };
    if (!payload.resources || typeof payload.resources !== 'object') payload.resources = {};
    payload.resources[resourceKey] = {
      ...(payload.resources[resourceKey] ?? {}),
      ...details,
      updatedAt: new Date().toISOString(),
    };
    payload.updatedAt = new Date().toISOString();
    writeJson(cdp.resourcesFile, payload);
    return payload.resources[resourceKey];
  };
  cdp.readSessionMetadata = () => readJson(cdp.sessionMetaFile, {});
  cdp.writeSessionMetadata = (patch) => {
    const current = readJson(cdp.sessionMetaFile, {}) ?? {};
    const next = {
      ...current,
      ...patch,
      updatedAt: new Date().toISOString(),
    };
    writeJson(cdp.sessionMetaFile, next);
    return next;
  };
  console.error(`[CDP_RUNNER] Output dir: ${outputDir}`);
  console.error(`[CDP_RUNNER] Session meta dir: ${sessionMetaDir}`);
  console.error(`[CDP_RUNNER] Connected - running ${scriptArg}`);

  // Node PM has no scoped networking; keep generated script fetch/WebSocket on localhost.
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

  registerCleanup(async () => {
    cdp.close();
    if (openedTabId && !KEEP_TAB) {
      const closed = await closeTab(openedTabId);
      console.error(`[CDP_RUNNER] Tab ${openedTabId} ${closed ? 'closed' : 'already gone'}`);
    }
  });

  const scriptPath = resolve(process.cwd(), scriptArg);
  if (!existsSync(scriptPath)) {
    console.error(`[CDP_RUNNER] Script not found: ${scriptPath}`);
    cdp.writeSessionMetadata({ lastRunStatus: 'error', lastError: `Script not found: ${scriptPath}` });
    finalizeRun('error', { error: `Script not found: ${scriptPath}` });
    await _cleanup?.();
    process.exit(1);
  }

  let mod;
  try {
    mod = await import(pathToFileURL(scriptPath).href);
  } catch (e) {
    console.error(`[CDP_RUNNER] Failed to load script: ${e.message}`);
    cdp.writeSessionMetadata({ lastRunStatus: 'error', lastError: e.message });
    finalizeRun('error', { error: e.message });
    await _cleanup?.();
    process.exit(1);
  }

  if (typeof mod.run !== 'function') {
    console.error('[CDP_RUNNER] Script must export: export async function run(cdp) { ... }');
    cdp.writeSessionMetadata({
      lastRunStatus: 'error',
      lastError: 'Script must export: export async function run(cdp) { ... }',
    });
    finalizeRun('error', { error: 'missing run(cdp) export' });
    await _cleanup?.();
    process.exit(1);
  }

  let exitCode = 0;
  try {
    await mod.run(cdp);
    cdp.writeSessionMetadata({ lastRunStatus: 'success' });
    finalizeRun('success');
    console.error('[CDP_RUNNER] Script completed successfully');
  } catch (e) {
    const isCdpError = /CDP error \[|CDP timeout/.test(e.message);
    if (isCdpError) {
      const methodMatch = e.message.match(/for:\s*(\S+)/) ?? e.message.match(/'([A-Z][a-zA-Z]+\.[a-zA-Z]+)'/);
      const method = methodMatch ? methodMatch[1] : 'unknown';
      console.log(`[CDP_RETRY_NEEDED] method=${method} error="${e.message}"`);
      console.log(`[CDP_RETRY_NEEDED] Fix: ensure the domain for "${method}" is enabled before calling it, check parameter names, and re-run.`);
      cdp.writeSessionMetadata({
        lastRunStatus: 'retry-needed',
        lastError: e.message,
        lastErrorMethod: method,
      });
      finalizeRun('retry-needed', { error: e.message, errorMethod: method });
      exitCode = 2;
    } else {
      console.error(`[CDP_RUNNER] Script error: ${e.message}`);
      if (e.stack) console.error(e.stack);
      cdp.writeSessionMetadata({
        lastRunStatus: 'error',
        lastError: e.message,
      });
      finalizeRun('error', { error: e.message });
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
