#!/usr/bin/env node
import { connectCDP } from './cdp-connection.mjs';

import { isAbsolute, join, relative, resolve, basename } from 'path';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'fs';
import { pathToFileURL, fileURLToPath } from 'url';
import { dirname } from 'path';
import { propagateOctocodeEnv } from './octocode-config.mjs';
import {
  applyMandatoryStealth,
  stealthEnabled,
  isAboutOrDataUrl,
} from './mandatory-stealth.mjs';

propagateOctocodeEnv({ cwd: process.cwd(), trusted: true });

function octocodeOutputBase() {
  const workspace = resolve(process.cwd(), '.octocode');
  mkdirSync(workspace, { recursive: true, mode: 0o700 });
  return workspace;
}

const OCTOCODE_OUTPUT_BASE = octocodeOutputBase();
function workspaceOutputPath(candidate, label) {
  const resolved = resolve(candidate);
  const rel = relative(OCTOCODE_OUTPUT_BASE, resolved);
  if (rel.startsWith('..') || isAbsolute(rel)) {
    console.error(
      `[CDP_RUNNER] ${label} must stay under ${OCTOCODE_OUTPUT_BASE}`
    );
    process.exit(2);
  }
  return resolved;
}
const ENV_SESSION_META_DIR = process.env.CDP_SESSION_META_DIR
  ? workspaceOutputPath(
      process.env.CDP_SESSION_META_DIR,
      'CDP_SESSION_META_DIR'
    )
  : null;
const ENV_OUTPUT_DIR = process.env.CDP_OUTPUT_DIR
  ? workspaceOutputPath(process.env.CDP_OUTPUT_DIR, 'CDP_OUTPUT_DIR')
  : null;
const argv = process.argv.slice(2);
const scriptArg = argv.find(
  a => !a.startsWith('--') && (a.endsWith('.mjs') || a.endsWith('.js'))
);
const getArg = (flag: string, def?: string) => {
  const i = argv.indexOf(flag);
  return i !== -1 && argv[i + 1] ? argv[i + 1] : def;
};
const hasFlag = flag => argv.includes(flag);

const PORT = getArg('--port', '9222');
const NEW_TAB = getArg('--new-tab', '');
const TARGET_ID = getArg('--target', '');
const TARGET_URL = getArg('--target-url', '');
const TARGET_TYPE = getArg('--target-type', '');
const TIMEOUT = Number(getArg('--timeout', '60000'));
const KEEP_TAB = hasFlag('--keep-tab');
const LIST_TARGETS = hasFlag('--list-targets');
const BROWSER = hasFlag('--browser');
const STRICT_TARGET = hasFlag('--strict-target');
function uniqueTarget(matches) {
  if (STRICT_TARGET && matches.length > 1)
    throw new Error(
      `Ambiguous target: ${matches.length} matches. Run targets and select --target <id>.`
    );
  return matches[0];
}
const VERBOSE = process.env.CDP_VERBOSE === '1';
if (hasFlag('--stealth')) process.env.CDP_STEALTH = '1';
if (hasFlag('--no-stealth')) process.env.CDP_NO_STEALTH = '1';
if (hasFlag('--no-reload')) process.env.CDP_STEALTH_NO_RELOAD = '1';

if (hasFlag('--help') || hasFlag('-h')) {
  console.error(
    '[CDP_RUNNER] Usage: node cdp-runner.mjs <script.mjs> [--port 9222] [--new-tab <url>] [--target <id>] [--target-url <pattern>] [--target-type <type>] [--list-targets] [--browser] [--keep-tab] [--stealth] [--no-reload] [--no-stealth]'
  );
  process.exit(0);
}

if (!Number.isSafeInteger(TIMEOUT) || TIMEOUT < 1) {
  console.error('[CDP_RUNNER] --timeout must be a positive integer');
  process.exit(2);
}

if (!scriptArg && !LIST_TARGETS) {
  console.error(
    '[CDP_RUNNER] Usage: node cdp-runner.mjs <script.mjs> [--port 9222] [--new-tab <url>] [--target <id>] [--target-url <pattern>] [--target-type <type>] [--list-targets] [--browser] [--keep-tab] [--stealth] [--no-reload] [--no-stealth]'
  );
  process.exit(1);
}

const [nodeMajor] = process.versions.node.split('.').map(Number);
if (nodeMajor < 24) {
  console.error(
    `[CDP_RUNNER] Node.js 24+ required (you have ${process.versions.node}). Native WebSocket is unavailable.`
  );
  process.exit(1);
}

function readJson(filePath, fallback = null) {
  try {
    return JSON.parse(readFileSync(filePath, 'utf8'));
  } catch {
    return fallback;
  }
}

function writeJson(filePath, value) {
  writeFileSync(filePath, `${JSON.stringify(value, null, 2)}\n`, {
    mode: 0o600,
  });
}

async function cdpHttp(path, method = 'GET') {
  const res = await fetch(`http://localhost:${PORT}${path}`, {
    method,
    signal: AbortSignal.timeout(5000),
  });
  if (!res.ok) throw new Error(`CDP HTTP ${res.status} for ${path}`);
  return res.json();
}

async function getVersion() {
  return cdpHttp('/json/version');
}
async function getTargets() {
  return cdpHttp('/json');
}
async function openTab(url) {
  return cdpHttp(`/json/new?${encodeURIComponent(url)}`, 'PUT');
}
async function activateTarget(id) {
  return cdpHttp(`/json/activate/${id}`);
}
async function closeTab(id) {
  try {
    const res = await fetch(`http://localhost:${PORT}/json/close/${id}`, {
      signal: AbortSignal.timeout(3000),
    });
    return res.ok;
  } catch {
    return false;
  }
}

const createSession = (url: string, targetInfo: Record<string, any>) =>
  connectCDP(url, { targetInfo, timeoutMs: TIMEOUT });

let _cleanup = null;
let _interrupted = null;
function registerCleanup(fn) {
  _cleanup = fn;
}

async function shutdown(signal) {
  console.error(`[CDP_RUNNER] ${signal} received - cleaning up...`);
  _interrupted?.(signal);
  if (_cleanup) {
    try {
      await _cleanup();
    } catch {}
  }
  process.exit(signal === 'SIGINT' ? 130 : 143);
}

process.on('SIGINT', () => shutdown('SIGINT'));
process.on('SIGTERM', () => shutdown('SIGTERM'));

async function main() {
  let version;
  try {
    version = await getVersion();
  } catch {
    console.error(
      `[CDP_RUNNER] Chrome not responding on port ${PORT}. Run open-browser.mjs first.`
    );
    process.exit(1);
  }
  if (VERBOSE) console.error(`[CDP_RUNNER] Chrome: ${version.Browser}`);
  const sessionMetaDir = ENV_SESSION_META_DIR
    ? ENV_SESSION_META_DIR
    : (() => {
        const dir = join(
          OCTOCODE_OUTPUT_BASE,
          'tmp',
          'chrome-devtools',
          'session-meta',
          `port-${PORT}`
        );
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
    const rows = targets.map(t => ({
      id: t.id,
      type: t.type,
      url: t.url,
      title: t.title,
    }));
    const encoded = JSON.stringify(rows, null, 2);
    // Small inventories stay convenient; large ones retain an immutable source
    // and a lossless query continuation instead of overflowing agent context.
    if (Buffer.byteLength(encoded) <= 18000) console.log(encoded);
    else {
      const file = join(
        sessionMetaDir,
        `targets-${Date.now()}-${Math.random().toString(36).slice(2)}.json`
      );
      writeJson(file, rows);
      console.log(
        JSON.stringify({
          count: rows.length,
          artifact: file,
          next: {
            continue: {
              command: process.execPath,
              args: [
                join(
                  dirname(fileURLToPath(import.meta.url)),
                  'evidence-query.mjs'
                ),
                '--file',
                file,
              ],
            },
          },
        })
      );
    }
    process.exit(0);
  }

  let targetWsUrl, targetInfo, openedTabId;
  let pendingNavigate = null;

  if (BROWSER) {
    if (NEW_TAB || TARGET_ID || TARGET_URL || TARGET_TYPE || stealthEnabled())
      throw new Error(
        '--browser cannot combine with page selection or --stealth'
      );
    const version = await getVersion();
    targetWsUrl = version.webSocketDebuggerUrl;
    targetInfo = {
      id: 'browser',
      type: 'browser',
      title: version.Browser,
      url: '',
    };
  } else if (NEW_TAB) {
    const tabUrl = NEW_TAB;
    const openUrl = !isAboutOrDataUrl(tabUrl) ? 'about:blank' : tabUrl;
    if (openUrl !== tabUrl) pendingNavigate = tabUrl;
    const tab = await openTab(openUrl);
    openedTabId = tab.id;
    targetWsUrl = tab.webSocketDebuggerUrl;
    targetInfo = { id: tab.id, url: tab.url, title: tab.title, type: tab.type };
    console.error(
      `[CDP_RUNNER] Opened new tab (${tab.id}) -> ${openUrl}${pendingNavigate ? ` (pending ${pendingNavigate})` : ''}`
    );
    await new Promise(r => setTimeout(r, 800));
  } else if (TARGET_ID) {
    const targets = await getTargets();
    const t = targets.find(x => x.id === TARGET_ID);
    if (!t) {
      console.error(`[CDP_RUNNER] Target ${TARGET_ID} not found`);
      process.exit(1);
    }
    targetWsUrl = t.webSocketDebuggerUrl;
    targetInfo = t;
    await activateTarget(TARGET_ID).catch(() => {});
  } else if (TARGET_URL) {
    const targets = await getTargets();
    const pool = targets.filter(t => t.type === (TARGET_TYPE || 'page'));
    const t = uniqueTarget(
      pool.filter(x => x.url && x.url.includes(TARGET_URL))
    );
    if (!t) {
      const available = targets.map(x => `  [${x.type}] ${x.url}`).join('\n');
      console.error(
        `[CDP_RUNNER] No target URL matching "${TARGET_URL}". Available targets:\n${available}`
      );
      process.exit(1);
    }
    targetWsUrl = t.webSocketDebuggerUrl;
    targetInfo = t;
    console.error(`[CDP_RUNNER] Matched target [${t.type}]: ${t.url}`);
  } else if (TARGET_TYPE) {
    const targets = await getTargets();
    const t = uniqueTarget(targets.filter(x => x.type === TARGET_TYPE));
    if (!t) {
      const available = [...new Set(targets.map(x => x.type))].join(', ');
      console.error(
        `[CDP_RUNNER] No target of type "${TARGET_TYPE}". Available types: ${available}`
      );
      process.exit(1);
    }
    targetWsUrl = t.webSocketDebuggerUrl;
    targetInfo = t;
    console.error(`[CDP_RUNNER] Matched target [${t.type}]: ${t.url}`);
  } else {
    const targets = await getTargets();
    const pages = targets.filter(t => t.type === 'page');
    if (pages.length === 0) {
      console.error(
        '[CDP_RUNNER] No page targets. Open a tab in Chrome first, or use --new-tab <url>'
      );
      process.exit(1);
    }
    const t = uniqueTarget(pages);
    targetWsUrl = t.webSocketDebuggerUrl;
    targetInfo = t;
    console.error(`[CDP_RUNNER] Using tab: ${t.url}`);
  }

  if (!targetWsUrl) {
    console.error('[CDP_RUNNER] Could not get WebSocket URL for target');
    process.exit(1);
  }

  const cdp: Awaited<ReturnType<typeof createSession>> & Record<string, any> =
    await createSession(targetWsUrl, targetInfo);
  cdp.foreground = async () => {
    if (targetInfo.type !== 'iframe') return cdp.send('Page.bringToFront');
    const browser = await createSession(version.webSocketDebuggerUrl, {
      type: 'browser',
    });
    try {
      const { targetInfos } = await browser.send('Target.getTargets');
      const byId = new Map<string, Record<string, any>>(
        targetInfos.map(info => [info.targetId, info])
      );
      let owner = byId.get(targetInfo.id);
      const seen = new Set();
      while (owner?.type === 'iframe' && !seen.has(owner.targetId)) {
        seen.add(owner.targetId);
        owner = byId.get(owner.parentId);
      }
      if (owner?.type !== 'page')
        throw new Error(
          'Cannot resolve iframe parent page from Target.getTargets parentId'
        );
      await browser.send('Target.activateTarget', { targetId: owner.targetId });
    } finally {
      browser.close();
    }
  };
  cdp.protocol = () => cdpHttp('/json/protocol');
  cdp.skillScriptsDir = dirname(fileURLToPath(import.meta.url));
  cdp.saveArtifact = (name, data, format = 'json') => {
    if (!name || basename(name) !== name || name === '.' || name === '..')
      throw new Error('Artifact name must be a filename');
    if (!['json', 'text', 'binary'].includes(format))
      throw new Error('Invalid artifact format');
    const file = join(cdp.outputDir, name);
    if (existsSync(file))
      throw new Error(
        'Artifact exists; use a new name to preserve continuations'
      );
    writeFileSync(
      file,
      format === 'json' ? JSON.stringify(data, null, 2) + '\n' : data,
      { mode: 0o600 }
    );
    const next = {
      continue: {
        command: process.execPath,
        args: [
          join(cdp.skillScriptsDir, 'artifact-query.mjs'),
          '--file',
          file,
          '--format',
          format,
        ],
      },
    };
    console.log(`[ARTIFACT] ${name} ${file}`);
    console.log(`[NEXT] ${JSON.stringify(next)}`);
    return { file, next };
  };

  const outputDir = ENV_OUTPUT_DIR
    ? ENV_OUTPUT_DIR
    : (() => {
        const ts = new Date().toISOString().slice(0, 19).replace(/[:T]/g, '-');
        const runs = join(OCTOCODE_OUTPUT_BASE, 'tmp', 'chrome-devtools');
        mkdirSync(runs, { recursive: true });
        for (let n = 1; ; n++) {
          const dir = join(runs, n === 1 ? ts : `${ts}-${n}`);
          try {
            mkdirSync(dir);
            return dir;
          } catch (e) {
            if (e.code !== 'EEXIST') throw e;
          }
        }
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
      via: NEW_TAB
        ? 'new-tab'
        : TARGET_ID
          ? 'target'
          : TARGET_URL
            ? 'target-url'
            : TARGET_TYPE
              ? 'target-type'
              : 'first-page',
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

  _interrupted = signal => {
    cdp.writeSessionMetadata?.({
      lastRunStatus: 'interrupted',
      lastError: signal,
    });
    finalizeRun('interrupted', { signal });
  };
  cdp.outputDir = outputDir;
  cdp.sessionMetaDir = sessionMetaDir;
  cdp.sessionMetaFile = sessionMetaFile;
  cdp.targetSnapshotFile = targetSnapshotFile;
  cdp.resourcesFile = join(sessionMetaDir, 'resource-map.json');
  cdp.upsertResourceMap = (resourceKey, details) => {
    const payload = readJson(cdp.resourcesFile, {
      updatedAt: null,
      resources: {},
    }) ?? { updatedAt: null, resources: {} };
    if (!payload.resources || typeof payload.resources !== 'object')
      payload.resources = {};
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
  cdp.writeSessionMetadata = patch => {
    const current = readJson(cdp.sessionMetaFile, {}) ?? {};
    const next = {
      ...current,
      ...patch,
      updatedAt: new Date().toISOString(),
    };
    writeJson(cdp.sessionMetaFile, next);
    return next;
  };
  if (VERBOSE) {
    console.error(`[CDP_RUNNER] Output dir: ${outputDir}`);
    console.error(`[CDP_RUNNER] Session meta dir: ${sessionMetaDir}`);
    console.error(`[CDP_RUNNER] Connected - running ${scriptArg}`);
  }

  // Scripts use CDP over local Chrome only. Blocking arbitrary outbound
  // fetch/WebSocket keeps generated examples from becoming network clients;
  // use browser-discovered API replay outside this runner when needed.
  // Deliberate exception: sourcemap-resolver.mjs fetches .map files via
  // Node's http/https core modules (not globalThis.fetch), which this
  // override does not touch — required since source maps live at the
  // page's own real domain, not localhost.
  const _origFetch = globalThis.fetch;
  const _OrigWS = globalThis.WebSocket;
  function isLocalhost(url) {
    try {
      const h = new URL(String(url)).hostname;
      return h === 'localhost' || h === '127.0.0.1' || h === '::1';
    } catch {
      return false;
    }
  }
  globalThis.fetch = function restrictedFetch(input, init) {
    const url =
      typeof input === 'string'
        ? input
        : input instanceof URL
          ? input.href
          : (input?.url ?? '');
    if (!isLocalhost(url)) {
      throw new Error(
        `[SANDBOX] fetch blocked: only localhost allowed (attempted: ${url})`
      );
    }
    return _origFetch(input, init);
  };
  globalThis.WebSocket = class RestrictedWebSocket extends _OrigWS {
    constructor(url, ...args) {
      if (!isLocalhost(url)) {
        throw new Error(
          `[SANDBOX] WebSocket blocked: only localhost allowed (attempted: ${url})`
        );
      }
      super(url, ...args);
    }
  };

  registerCleanup(async () => {
    cdp.close();
    if (openedTabId && !KEEP_TAB) {
      const closed = await closeTab(openedTabId);
      console.error(
        `[CDP_RUNNER] Tab ${openedTabId} ${closed ? 'closed' : 'already gone'}`
      );
    }
  });

  const scriptPath = resolve(process.cwd(), scriptArg);
  if (!existsSync(scriptPath)) {
    console.error(`[CDP_RUNNER] Script not found: ${scriptPath}`);
    cdp.writeSessionMetadata({
      lastRunStatus: 'error',
      lastError: `Script not found: ${scriptPath}`,
    });
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
    console.error(
      '[CDP_RUNNER] Script must export: export async function run(cdp) { ... }'
    );
    cdp.writeSessionMetadata({
      lastRunStatus: 'error',
      lastError: 'Script must export: export async function run(cdp) { ... }',
    });
    finalizeRun('error', { error: 'missing run(cdp) export' });
    await _cleanup?.();
    process.exit(1);
  }

  try {
    if (stealthEnabled())
      await applyMandatoryStealth(cdp, {
        navigateUrl: pendingNavigate ?? undefined,
      });
    if (pendingNavigate) {
      console.error(`[CDP_RUNNER] Navigating to ${pendingNavigate}`);
      await cdp.send('Page.enable');
      let onLoad;
      const loaded = new Promise(resolve => {
        onLoad = resolve;
        cdp.on('Page.loadEventFired', onLoad);
      });
      let timer;
      try {
        const navigation = await cdp.send('Page.navigate', {
          url: pendingNavigate,
        });
        if (navigation.errorText)
          throw new Error(`Navigation failed: ${navigation.errorText}`);
        const completed = await Promise.race([
          loaded.then(() => true),
          new Promise(resolve => {
            timer = setTimeout(() => resolve(false), 15000);
          }),
        ]);
        if (!completed)
          console.log(
            '[FINDING] PAGE_LOAD_TIMEOUT load event not observed within 15000ms; page may still be rendering'
          );
      } finally {
        clearTimeout(timer);
        cdp.off('Page.loadEventFired', onLoad);
      }
      const location = await cdp.send('Runtime.evaluate', {
        expression: 'location.href',
        returnByValue: true,
      });
      const currentUrl = location.result?.value || pendingNavigate;
      cdp.targetInfo = { ...cdp.targetInfo, url: currentUrl };
      cdp.writeSessionMetadata({
        currentTarget: { ...baseMeta.currentTarget, url: currentUrl },
      });
    }
  } catch (stealthErr) {
    console.error(`[CDP_RUNNER] ${stealthErr.message}`);
    cdp.writeSessionMetadata({
      lastRunStatus: 'error',
      lastError: stealthErr.message,
    });
    finalizeRun('error', { error: stealthErr.message });
    await _cleanup?.();
    process.exit(1);
  }

  let exitCode = 0;
  try {
    await mod.run(cdp);
    exitCode = Number(process.exitCode) || 0;
    const status = exitCode === 0 ? 'success' : 'error';
    cdp.writeSessionMetadata({ lastRunStatus: status, lastExitCode: exitCode });
    finalizeRun(status, { exitCode });
    if (VERBOSE) console.error('[CDP_RUNNER] Script completed successfully');
  } catch (e) {
    const isCdpError = /CDP error \[|CDP timeout/.test(e.message);
    if (isCdpError) {
      const methodMatch =
        e.message.match(/for:\s*(\S+)/) ??
        e.message.match(/'([A-Z][a-zA-Z]+\.[a-zA-Z]+)'/);
      const method = methodMatch ? methodMatch[1] : 'unknown';
      console.log(`[CDP_RETRY_NEEDED] method=${method} error="${e.message}"`);
      console.log(
        `[CDP_RETRY_NEEDED] Inspect "${method}", its parameters, enabled domains and target readiness before retrying.`
      );
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

  if (process.exitCode && exitCode === 0) {
    exitCode = Number(process.exitCode);
    cdp.writeSessionMetadata({
      lastRunStatus: 'error',
      lastExitCode: exitCode,
    });
    finalizeRun('error', { exitCode });
  }
  process.exit(exitCode);
}

main().catch(async e => {
  console.error('[CDP_RUNNER_FATAL]', e.message);
  if (_cleanup) {
    try {
      await _cleanup();
    } catch {}
  }
  process.exit(1);
});
