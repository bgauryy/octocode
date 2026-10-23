#!/usr/bin/env node
import { connectCDP } from './cdp-connection.mjs';

import { isAbsolute, join, relative, resolve } from 'path';
import { existsSync } from 'fs';
import { pathToFileURL } from 'url';
import { propagateOctocodeEnv } from './octocode-config.mjs';
import { flagArgs } from './cli-flags.mjs';
import { octocodeOutputBase } from './engine-io.mjs';
import { applyMandatoryStealth, stealthEnabled } from './mandatory-stealth.mjs';
import { restrictNetworkToLocalhost } from './cdp-runner-guard.mjs';
import {
  attachSaveArtifact,
  attachSessionStore,
  createRunDir,
  readJson,
  sessionMetaDir as sessionMetaDirFor,
  startRunHistory,
  writeJson,
  writeTargetSnapshot,
} from './cdp-runner-session.mjs';
import {
  attachForeground,
  chromeEndpoint,
  listTargets,
  navigateTab,
  selectTarget,
  selectionVia,
} from './cdp-runner-targets.mjs';

propagateOctocodeEnv({ cwd: process.cwd(), trusted: true });

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
const { getArg, hasFlag } = flagArgs(argv);

const PORT = getArg('--port', '9222');
const NEW_TAB = getArg('--new-tab', '');
const TARGET_ID = getArg('--target', '');
const TARGET_URL = getArg('--target-url', '');
const TARGET_TYPE = getArg('--target-type', '');
const TIMEOUT = Number(getArg('--timeout', '60000'));
const KEEP_TAB = hasFlag('--keep-tab');
const LIST_TARGETS = hasFlag('--list-targets');
const SELECTION = {
  browser: hasFlag('--browser'),
  newTab: NEW_TAB,
  targetId: TARGET_ID,
  targetUrl: TARGET_URL,
  targetType: TARGET_TYPE,
  strict: hasFlag('--strict-target'),
};
const VERBOSE = process.env.CDP_VERBOSE === '1';
if (hasFlag('--stealth')) process.env.CDP_STEALTH = '1';
if (hasFlag('--no-stealth')) process.env.CDP_NO_STEALTH = '1';
if (hasFlag('--no-reload')) process.env.CDP_STEALTH_NO_RELOAD = '1';

const USAGE =
  '[CDP_RUNNER] Usage: node cdp-runner.mjs <script.mjs> [--port 9222] [--new-tab <url>] [--target <id>] [--target-url <pattern>] [--target-type <type>] [--list-targets] [--browser] [--keep-tab] [--stealth] [--no-reload] [--no-stealth]';
if (hasFlag('--help') || hasFlag('-h')) {
  console.error(USAGE);
  process.exit(0);
}

if (!Number.isSafeInteger(TIMEOUT) || TIMEOUT < 1) {
  console.error('[CDP_RUNNER] --timeout must be a positive integer');
  process.exit(2);
}

if (!scriptArg && !LIST_TARGETS) {
  console.error(USAGE);
  process.exit(1);
}

const [nodeMajor] = process.versions.node.split('.').map(Number);
if (nodeMajor < 24) {
  console.error(
    `[CDP_RUNNER] Node.js 24+ required (you have ${process.versions.node}). Native WebSocket is unavailable.`
  );
  process.exit(1);
}

const chrome = chromeEndpoint(PORT);

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
    version = await chrome.version();
  } catch {
    console.error(
      `[CDP_RUNNER] Chrome not responding on port ${PORT}. Run open-browser.mjs first.`
    );
    process.exit(1);
  }
  if (VERBOSE) console.error(`[CDP_RUNNER] Chrome: ${version.Browser}`);
  const sessionMetaDir = sessionMetaDirFor(
    OCTOCODE_OUTPUT_BASE,
    ENV_SESSION_META_DIR,
    PORT
  );
  const sessionMetaFile = join(sessionMetaDir, 'session-metadata.json');
  const targetSnapshotFile = join(sessionMetaDir, 'targets-latest.json');

  if (LIST_TARGETS) {
    await listTargets(
      chrome,
      {
        port: PORT,
        browser: version.Browser,
        session: { sessionMetaDir, sessionMetaFile, targetSnapshotFile },
      },
      line => console.log(line)
    );
    process.exit(0);
  }

  const selected = await selectTarget(chrome, SELECTION, line =>
    console.error(line)
  );
  if (selected.error) {
    console.error(selected.error);
    process.exit(1);
  }
  const { targetWsUrl, targetInfo } = selected;
  const openedTabId = selected.openedTabId;
  const pendingNavigate = selected.pendingNavigate ?? null;

  if (!targetWsUrl) {
    console.error('[CDP_RUNNER] Could not get WebSocket URL for target');
    process.exit(1);
  }

  const cdp: Awaited<ReturnType<typeof createSession>> & Record<string, any> =
    await createSession(targetWsUrl, targetInfo);
  // A tab opened on about: or data: is usable once its document has loaded.
  if (openedTabId)
    await cdp
      .send(
        'Runtime.evaluate',
        {
          expression:
            "document.readyState === 'complete' || new Promise(resolve => addEventListener('load', resolve, { once: true }))",
          awaitPromise: true,
        },
        undefined,
        { timeoutMs: 15000 }
      )
      .catch(error =>
        console.error(
          `[CDP_RUNNER] New tab load not confirmed: ${error.message}`
        )
      );
  attachForeground(cdp, targetInfo, () =>
    createSession(version.webSocketDebuggerUrl, { type: 'browser' })
  );
  cdp.protocol = chrome.protocol;
  cdp.skillScriptsDir = import.meta.dirname;
  attachSaveArtifact(cdp, line => console.log(line));

  const outputDir = ENV_OUTPUT_DIR || createRunDir(OCTOCODE_OUTPUT_BASE);
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
      via: selectionVia(SELECTION),
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

  const currentTargets = await chrome.targets().catch(() => []);
  writeTargetSnapshot(targetSnapshotFile, {
    capturedAt: nowIso,
    port: PORT,
    targets: currentTargets,
    attachedId: targetInfo.id,
  });

  const finalizeRun = startRunHistory(runLogFile, {
    startedAt: nowIso,
    script: scriptArg,
    outputDir,
    target: baseMeta.currentTarget,
  });

  _interrupted = signal => {
    cdp.writeSessionMetadata?.({
      lastRunStatus: 'interrupted',
      lastError: signal,
    });
    finalizeRun('interrupted', { signal });
  };
  cdp.outputDir = outputDir;
  attachSessionStore(cdp, {
    sessionMetaDir,
    sessionMetaFile,
    targetSnapshotFile,
  });
  if (VERBOSE) {
    console.error(`[CDP_RUNNER] Output dir: ${outputDir}`);
    console.error(`[CDP_RUNNER] Session meta dir: ${sessionMetaDir}`);
    console.error(`[CDP_RUNNER] Connected - running ${scriptArg}`);
  }

  restrictNetworkToLocalhost();

  registerCleanup(async () => {
    cdp.close();
    if (openedTabId && !KEEP_TAB) {
      const closed = await chrome.close(openedTabId);
      console.error(
        `[CDP_RUNNER] Tab ${openedTabId} ${closed ? 'closed' : 'already gone'}`
      );
    }
  });

  // A run that cannot start: record why, release the tab and exit 1.
  const abort = async (message, lastError, error = lastError) => {
    console.error(`[CDP_RUNNER] ${message}`);
    cdp.writeSessionMetadata({ lastRunStatus: 'error', lastError });
    finalizeRun('error', { error });
    await _cleanup?.();
    process.exit(1);
  };
  const scriptPath = resolve(process.cwd(), scriptArg);
  if (!existsSync(scriptPath))
    await abort(
      `Script not found: ${scriptPath}`,
      `Script not found: ${scriptPath}`
    );

  let mod;
  try {
    mod = await import(pathToFileURL(scriptPath).href);
  } catch (e) {
    await abort(`Failed to load script: ${e.message}`, e.message);
  }

  if (typeof mod.run !== 'function') {
    const missing =
      'Script must export: export async function run(cdp) { ... }';
    await abort(missing, missing, 'missing run(cdp) export');
  }

  try {
    if (stealthEnabled())
      await applyMandatoryStealth(cdp, {
        navigateUrl: pendingNavigate ?? undefined,
      });
    if (pendingNavigate) {
      console.error(`[CDP_RUNNER] Navigating to ${pendingNavigate}`);
      const currentUrl = await navigateTab(cdp, pendingNavigate, line =>
        console.log(line)
      );
      cdp.targetInfo = { ...cdp.targetInfo, url: currentUrl };
      cdp.writeSessionMetadata({
        currentTarget: { ...baseMeta.currentTarget, url: currentUrl },
      });
    }
  } catch (stealthErr) {
    await abort(stealthErr.message, stealthErr.message);
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
