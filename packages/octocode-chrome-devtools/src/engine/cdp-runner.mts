#!/usr/bin/env node
import { connectCDP, type Payload } from './cdp-connection.mjs';
import { errorMessage } from './error-message.mjs';

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
  readJsonObject,
  reportRunError,
  sessionMetaDir as sessionMetaDirFor,
  startRunHistory,
  writeJson,
  writeTargetSnapshot,
} from './cdp-runner-session.mjs';
import {
  attachForeground,
  chromeEndpoint,
  confirmTabLoaded,
  listTargets,
  navigateTab,
  selectTarget,
  selectionVia,
} from './cdp-runner-targets.mjs';

propagateOctocodeEnv({ cwd: process.cwd(), trusted: true });

const OCTOCODE_OUTPUT_BASE = octocodeOutputBase();
function workspaceOutputPath(candidate: string, label: string) {
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
// getArg returns the default whenever the flag has no value, so these are never undefined.

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

const createSession = (url: string, targetInfo: Payload) =>
  connectCDP(url, { targetInfo, timeoutMs: TIMEOUT });

let _cleanup: (() => Promise<void>) | null = null;
let _interrupted: ((signal: string) => void) | null = null;
function registerCleanup(fn: () => Promise<void>) {
  _cleanup = fn;
}

async function shutdown(signal: string) {
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

  // A run without a script never gets here unless it lists targets (checked at startup).
  if (LIST_TARGETS || !scriptArg) {
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
  if (selected.error !== undefined) {
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

  const connected = await createSession(targetWsUrl, targetInfo);
  const cdp = attachSessionStore(
    attachSaveArtifact(
      Object.assign(
        attachForeground(connected, targetInfo, () =>
          createSession(version.webSocketDebuggerUrl, { type: 'browser' })
        ),
        { protocol: chrome.protocol, skillScriptsDir: import.meta.dirname }
      ),
      line => console.log(line)
    ),
    { sessionMetaDir, sessionMetaFile, targetSnapshotFile }
  );
  if (openedTabId) await confirmTabLoaded(cdp, line => console.error(line));

  const outputDir = ENV_OUTPUT_DIR || createRunDir(OCTOCODE_OUTPUT_BASE);
  const runLogFile = join(sessionMetaDir, 'run-history.json');
  const nowIso = new Date().toISOString();
  const baseMeta = {
    ...readJsonObject(sessionMetaFile),
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
    cdp.writeSessionMetadata({
      lastRunStatus: 'interrupted',
      lastError: signal,
    });
    finalizeRun('interrupted', { signal });
  };
  cdp.outputDir = outputDir;
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
  const abort = async (
    message: string,
    lastError: string,
    error = lastError
  ): Promise<never> => {
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

  let mod: { run?: unknown };
  try {
    mod = await import(pathToFileURL(scriptPath).href);
  } catch (e) {
    await abort(`Failed to load script: ${errorMessage(e)}`, errorMessage(e));
    return;
  }
  const { run } = mod;
  if (typeof run !== 'function') {
    const missing =
      'Script must export: export async function run(cdp) { ... }';
    await abort(missing, missing, 'missing run(cdp) export');
    return;
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
    await abort(errorMessage(stealthErr), errorMessage(stealthErr));
  }

  let exitCode = 0;
  try {
    // Called on the module, as `mod.run(cdp)`, so a run that reads `this` keeps working.
    await run.call(mod, cdp);
    exitCode = Number(process.exitCode) || 0;
    const status = exitCode === 0 ? 'success' : 'error';
    cdp.writeSessionMetadata({ lastRunStatus: status, lastExitCode: exitCode });
    finalizeRun(status, { exitCode });
    if (VERBOSE) console.error('[CDP_RUNNER] Script completed successfully');
  } catch (e) {
    exitCode = reportRunError(cdp, finalizeRun, e, console);
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

main().catch(async (e: unknown) => {
  console.error('[CDP_RUNNER_FATAL]', errorMessage(e));
  if (_cleanup) {
    try {
      await _cleanup();
    } catch {}
  }
  process.exit(1);
});
