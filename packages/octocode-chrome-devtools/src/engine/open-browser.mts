#!/usr/bin/env node

import { stopProxyFor } from './cdp-proxy.mjs';
import { spawn } from 'child_process';
import { platform } from 'os';
import { join } from 'path';
import {
  closeSync,
  existsSync,
  openSync,
  readFileSync,
  writeFileSync,
  rmSync,
  mkdirSync,
} from 'fs';
import { propagateOctocodeEnv } from './octocode-config.mjs';
import { browserMajorVersion, planRealProfile } from './real-profile.mjs';
import { flagArgs } from './cli-flags.mjs';
import { openSelectedTab } from './open-tab.mjs';
import { disposeAllGroupContexts } from './group-tabs.mjs';
import {
  normalizeOrigin,
  originAndPath,
  originViolation,
  readSessionMeta,
  removeSessionMeta,
  updateSessionMeta,
} from './session-meta.mjs';
import { chromeEndpoint } from './cdp-runner-targets.mjs';
import { OPEN_BROWSER_HELP } from './open-browser-help.mjs';
import {
  emulationReply,
  parseEmulation,
  type Emulation,
} from './emulation-state.mjs';
import { errorMessage } from './error-message.mjs';
import { urlSchemeRefusal } from './cdp-policy.mjs';
import {
  findChrome,
  removeDirWithRetry,
  waitForExit,
} from './browser-process.mjs';
import {
  discoverListenerSession,
  processMatchesTrackedSession,
} from './browser-session.mjs';
import {
  checkRunning,
  profileDirIn,
  readSession,
  sessionFileIn,
  stateDirFor,
  isolatedProfileNote,
  recordActiveSession,
} from './session-registry.mjs';
import {
  appliedLaunchFlags,
  chromeLaunchArgs,
  effectiveProxy,
  isNoSandboxFailure,
  launchFeatures,
  validateChromeFlags,
  NO_SANDBOX_HINT,
  loadProxyConfig,
} from './browser-launch-config.mjs';

const argv = process.argv.slice(2);
const { getArg, hasFlag } = flagArgs(argv);
// getArg returns the default whenever the flag has no value, so these are never undefined.

if (hasFlag('--help') || hasFlag('-h')) {
  console.log(OPEN_BROWSER_HELP);
  process.exit(0);
}

const PORT = getArg('--port', '9222');
const PROFILE = getArg('--profile', 'Default');
const URL_ARG = getArg('--url', '');
const HEADLESS = hasFlag('--headless');
const REAL_PROFILE = hasFlag('--profile');
const CLEANUP = hasFlag('--cleanup');
const DRY_RUN = hasFlag('--dry-run');
const CHROME_PATH = getArg('--chromePath', '');
const WINDOW_SIZE = getArg('--windowSize', HEADLESS ? '1280x720' : '');
const ENABLE_FEATURES = getArg('--enableFeatures', '');
const DISABLE_FEATURES = getArg('--disableFeatures', '');
const BLINK_FEATURES = getArg('--blinkFeatures', '');
const CHROME_FLAGS = getArg('--chrome-flags', '')
  .split(',')
  .map(flag => flag.trim())
  .filter(Boolean);
const USER_AGENT = getArg('--userAgent', '');
const PROXY_SERVER = getArg('--proxyServer', '');
const PROXY_BYPASS_LIST = getArg('--proxyBypassList', '');
const PROXY_PAC_URL = getArg('--proxyPacUrl', '');
const CONFIG_PATH = getArg('--config', '');
const ATTACH = hasFlag('--attach');
const ACKNOWLEDGE = hasFlag('--acknowledge');
const NO_SANDBOX = hasFlag('--no-sandbox');
const ALLOW_ORIGIN_ARG = getArg('--allow-origin', '');

function ok(payload: Record<string, unknown>) {
  console.log(JSON.stringify(payload));
}
function err(message: string): never {
  console.log(JSON.stringify({ status: 'ERROR', message }));
  process.exit(1);
}

// PORT names the session file and the profile directory that cleanup deletes.
if (!/^\d+$/.test(PORT) || Number(PORT) < 1 || Number(PORT) > 65535)
  err('--port needs a positive integer <= 65535');

propagateOctocodeEnv({ cwd: process.cwd(), trusted: true });

// Every isolated profile and session file lives at
// <workspace>/.octocode/octocode-chrome-devtools/tmp/browser-state/, keyed by port.
const WORKSPACE = process.cwd();
const CHROME_STATE_DIR = stateDirFor(WORKSPACE);
const SESSION_FILE = sessionFileIn(CHROME_STATE_DIR, PORT);
const HEADLESS_PROFILE_DIR = profileDirIn(CHROME_STATE_DIR, PORT);

function writeSession(pid: number | undefined, wsUrl?: string) {
  writeFileSync(
    SESSION_FILE,
    JSON.stringify({
      pid,
      port: PORT,
      profileDir: HEADLESS_PROFILE_DIR,
      headless: HEADLESS,
      isolated: usingIsolatedProfile,
      startedAt: Date.now(),
      chromePath,
      wsUrl,
    }),
    { mode: 0o600 }
  );
}

/** Disposes the private groups' browser contexts on this port; returns how many. */
async function disposeGroupContexts(): Promise<number> {
  if (DRY_RUN) return 0;
  try {
    return await disposeAllGroupContexts(PORT, WORKSPACE);
  } catch {
    return 0;
  }
}

async function cleanupSession() {
  // A proxy forwards to this browser; it must not outlive it. A dry run changes nothing.
  if (!DRY_RUN) await stopProxyFor(PORT, CHROME_STATE_DIR);
  const disposedContexts = await disposeGroupContexts();
  let session = readSession(SESSION_FILE);
  let sessionFile = SESSION_FILE;
  let owner: Record<string, unknown> = { workspace: WORKSPACE, sessionFile };
  if (!session) {
    const found = await discoverListenerSession(null, PORT);
    if (found.session === undefined) {
      console.error(
        `[BROWSER] No tracked session found for port ${PORT} (${found.reason})`
      );
      if (disposedContexts)
        updateSessionMeta(
          PORT,
          { contexts: undefined, groups: undefined },
          WORKSPACE
        );
      ok({
        status: 'NO_TRACKED_SESSION',
        port: PORT,
        disposedContexts,
        reason: found.reason,
        listenerPid: found.listenerPid,
        workspace: found.workspace,
      });
      return;
    }
    session = found.session;
    sessionFile = found.sessionFile;
    owner = {
      workspace: found.workspace,
      sessionFile,
      discoveredFromListener: true,
    };
    console.error(
      `[BROWSER] Port ${PORT} belongs to the session of workspace ${found.workspace}`
    );
  }
  if (String(session.port) !== String(PORT)) {
    err(
      `Tracked session port mismatch: expected ${PORT}, found ${session.port}`
    );
  }
  if (!processMatchesTrackedSession(session)) {
    console.error(
      `[BROWSER] Refusing to kill pid=${session.pid}; it does not match the tracked CDP port/profile`
    );
    if (DRY_RUN) {
      ok({
        status: 'CLEANUP_DRY_RUN',
        port: PORT,
        pid: session.pid,
        matchesTrackedSession: false,
        wouldKill: false,
        ...owner,
      });
      return;
    }
    try {
      rmSync(sessionFile, { force: true });
    } catch {}
    removeSessionMeta(PORT, WORKSPACE);
    ok({
      status: 'STALE_SESSION_REMOVED',
      port: PORT,
      disposedContexts,
      ...owner,
    });
    return;
  }

  if (DRY_RUN) {
    ok({
      status: 'CLEANUP_DRY_RUN',
      port: PORT,
      pid: session.pid,
      matchesTrackedSession: true,
      wouldKill: true,
      profileDir: session.profileDir,
      ...owner,
    });
    return;
  }

  try {
    process.kill(session.pid, 'SIGTERM');
    console.error(`[BROWSER] Sent SIGTERM to Chrome pid=${session.pid}`);
  } catch {
    console.error(`[BROWSER] Process pid=${session.pid} already gone`);
  }
  const exited = await waitForExit(session.pid);
  if (!exited) {
    try {
      process.kill(session.pid, 'SIGKILL');
      console.error(`[BROWSER] Sent SIGKILL to Chrome pid=${session.pid}`);
    } catch {}
    await waitForExit(session.pid, 1000);
  }

  if (session.profileDir && existsSync(session.profileDir)) {
    try {
      await removeDirWithRetry(session.profileDir);
      console.error(`[BROWSER] Removed profile: ${session.profileDir}`);
    } catch (e) {
      console.error(`[BROWSER] Could not remove profile: ${errorMessage(e)}`);
    }
  }

  try {
    rmSync(sessionFile, { force: true });
  } catch {}
  removeSessionMeta(PORT, WORKSPACE);
  console.error('[BROWSER] Session cleaned up');
  ok({
    status: 'CLEANED_UP',
    port: PORT,
    pid: session.pid,
    disposedContexts,
    ...owner,
  });
}

if (CLEANUP) {
  await cleanupSession();
  process.exit(0);
}

if (REAL_PROFILE && HEADLESS)
  err(
    '--profile drives the real profile in a visible window; drop --headless, or drop --profile for an isolated headless session.'
  );

const proxy = effectiveProxy(
  {
    server: PROXY_SERVER,
    bypassList: PROXY_BYPASS_LIST,
    pacUrl: PROXY_PAC_URL,
  },
  loadProxyConfig(CONFIG_PATH, {
    fail: err,
    warn: line => console.error(line),
  })
);
const proxyRequested = Boolean(proxy.server || proxy.pacUrl);
// Refused here too, so a denylisted flag fails even when the port already has a browser.
try {
  validateChromeFlags(CHROME_FLAGS);
} catch (e) {
  err(errorMessage(e));
}
if (hasFlag('--chrome-flags') && !CHROME_FLAGS.length)
  err('--chrome-flags needs a comma-separated list of --name or --name=value');
const typedLaunchFlags = [
  CHROME_FLAGS.length && '--chrome-flags',
  DISABLE_FEATURES && '--disableFeatures',
  BLINK_FEATURES && '--blinkFeatures',
].filter(Boolean);

let allowOrigins: string[] | undefined;
try {
  allowOrigins = ALLOW_ORIGIN_ARG
    ? [...new Set(ALLOW_ORIGIN_ARG.split(',').map(normalizeOrigin))]
    : undefined;
} catch (e) {
  err(errorMessage(e));
}
if (hasFlag('--allow-origin') && !allowOrigins)
  err('--allow-origin needs a comma-separated list of origins');
if (ATTACH) {
  if (!hasFlag('--port')) err('--attach needs --port');
  if (!ACKNOWLEDGE)
    err(
      "--attach needs --acknowledge (acknowledge: true): attaching exposes the browser's logged-in pages and cookies; only with the user's explicit consent"
    );
  const launching = [
    HEADLESS && '--headless',
    REAL_PROFILE && '--profile',
    CHROME_PATH && '--chromePath',
    NO_SANDBOX && '--no-sandbox',
    proxyRequested && 'proxy settings',
    ENABLE_FEATURES && '--enableFeatures',
    USER_AGENT && '--userAgent',
    hasFlag('--windowSize') && '--windowSize',
    ...typedLaunchFlags,
  ].filter(Boolean);
  if (launching.length)
    err(
      `--attach never launches a browser, so ${launching.join(', ')} cannot apply`
    );
}

let emulation: Emulation | undefined;
if (hasFlag('--emulation')) {
  try {
    emulation = parseEmulation(getArg('--emulation', ''), {
      acknowledged: ACKNOWLEDGE,
    });
  } catch (e) {
    console.log(JSON.stringify({ status: 'ERROR', message: errorMessage(e) }));
    process.exit(2);
  }
}
/** What the session meta holds after this call: the new object, nothing for a reset, no change when absent. */
const emulationPatch =
  emulation === undefined
    ? {}
    : { emulation: emulation.reset === true ? undefined : emulation };
const emulationReport = emulation
  ? { emulation: emulationReply(emulation) }
  : {};

async function openTab(
  url: string,
  allowed?: readonly string[]
): Promise<string> {
  try {
    return await openSelectedTab(
      PORT,
      url,
      line => console.error(line),
      allowed
    );
  } catch (e) {
    return err(
      `Chrome on port ${PORT} is reused, but opening ${url} failed: ${errorMessage(e)}`
    );
  }
}

const existing = await checkRunning(PORT);
if (ATTACH && !existing)
  err(
    `Nothing is listening on port ${PORT}: --attach connects only to a Chrome already running on that exact port. Nothing was launched and no other port was scanned. Start Chrome with --remote-debugging-port=${PORT}, or call open without attach to launch an isolated browser.`
  );
// The list a navigation of this call is checked against: a reused browser keeps its own list
// unless this call sets one; a fresh launch starts from this call's list only.
const effectiveOrigins = existing
  ? (allowOrigins ?? readSessionMeta(PORT, WORKSPACE).allowOrigins)
  : allowOrigins;
if (URL_ARG) {
  const scheme = urlSchemeRefusal(URL_ARG, '--url', null);
  if (scheme) err(scheme);
  const violation = originViolation(URL_ARG, effectiveOrigins);
  if (violation) err(`${URL_ARG}: ${violation}`);
}

/**
 * Chrome navigates the launch URL itself, so a redirect can leave the allowlist after the
 * pre-launch check. Wait for the first page to settle, then close it and fail if it left.
 */
async function enforceLaunchUrlOrigin() {
  let last = '';
  for (let tries = 0; tries < 20; tries++) {
    await new Promise(r => setTimeout(r, 250));
    const pages = (
      await fetch(`http://127.0.0.1:${PORT}/json/list`)
        .then(
          r => r.json() as Promise<{ id: string; type: string; url: string }[]>
        )
        .catch(() => [])
    ).filter(t => t.type === 'page');
    const page = pages[0];
    if (!page) continue;
    const settled =
      page.url === last && !/^(about:blank|chrome:)/.test(page.url);
    last = page.url;
    if (!settled) continue;
    const violation = originViolation(page.url, allowOrigins);
    if (violation) {
      await fetch(`http://127.0.0.1:${PORT}/json/close/${page.id}`).catch(
        () => undefined
      );
      err(
        `The launch URL redirected to ${page.url}, outside the allowlist; the tab was closed. ${violation}`
      );
    }
    return;
  }
}

/** What an attached endpoint is, from what Chrome and this workspace's session file say; never a guess. */
function profileKind(browser: string, isolatedSession: boolean) {
  if (isolatedSession) return 'isolated (launched by octocode open)';
  if (browser.startsWith('HeadlessChrome')) return 'headless isolated';
  return 'unknown (a real profile cannot be told from a separate one over CDP)';
}

async function attachedFacts(
  info: { Browser: string },
  isolatedSession: boolean
) {
  const rows = await chromeEndpoint(PORT)
    .targets()
    .catch(() => []);
  const pages = rows.filter(row => row.type === 'page');
  return {
    attached: true,
    browserVersion: info.Browser,
    profile: profileKind(info.Browser, isolatedSession),
    openTabCount: pages.length,
    openTabs: pages.map(row => originAndPath(row.url)),
  };
}

if (existing) {
  // Report which workspace's isolated session owns the reused browser, if any.
  const owner = await discoverListenerSession(existing, PORT);
  // A listener this package did not launch holds someone's logged-in pages and cookies: attach only on consent.
  if (!owner.session && !ATTACH)
    err(
      `A browser this package did not launch is listening on port ${PORT}. Attaching exposes its logged-in pages and cookies: ask the user, then retry with attach: true and acknowledge: true.`
    );
  const notApplied = [
    proxyRequested && 'proxy settings',
    ENABLE_FEATURES && '--enableFeatures',
    REAL_PROFILE && '--profile',
    USER_AGENT && '--userAgent',
    hasFlag('--windowSize') && '--windowSize',
    NO_SANDBOX && '--no-sandbox',
    ...typedLaunchFlags,
  ].filter(Boolean);
  if (allowOrigins || emulation)
    updateSessionMeta(
      PORT,
      { ...(allowOrigins ? { allowOrigins } : {}), ...emulationPatch },
      WORKSPACE
    );
  // A reused browser keeps its tabs: the requested URL opens in a new tab that later commands
  // without a target use, and the result reports it.
  const targetId = URL_ARG
    ? await openTab(URL_ARG, effectiveOrigins)
    : undefined;
  const selectedId = targetId;
  // Commands without a port use this browser next, unless another workspace owns it.
  if (!owner.workspace || owner.workspace === WORKSPACE)
    recordActiveSession(CHROME_STATE_DIR, {
      port: PORT,
      pid: owner.session?.pid,
      wsUrl: existing.webSocketDebuggerUrl,
      reused: true,
      targetId: selectedId,
    });
  // Facts about the attached endpoint are read after the url tab opened: they are part of it.
  const attachedReply = ATTACH
    ? await attachedFacts(existing, Boolean(owner.session))
    : {};
  ok({
    status: 'BROWSER_READY',
    wsUrl: existing.webSocketDebuggerUrl,
    port: PORT,
    reused: true,
    ...attachedReply,
    ...emulationReport,
    ...(targetId ? { url: URL_ARG, targetId } : {}),
    browser: existing.Browser,
    ...(effectiveOrigins ? { allowOrigins: effectiveOrigins } : {}),
    ...(owner.session
      ? {
          isolated: true,
          sessionFile: owner.sessionFile,
          workspace: owner.workspace,
        }
      : {}),
    proxyConfigured: false,
    proxyRequested,
    enableFeaturesRequested: ENABLE_FEATURES || undefined,
    ...(notApplied.length ? { launchFlagsIgnored: true } : {}),
    warning: notApplied.length
      ? `Existing Chrome was reused; ${notApplied.join(', ')} apply only when launching a fresh browser and were not applied. Run cleanup or use another port for a fresh session.`
      : undefined,
  });
  process.exit(0);
}

const chromePath = CHROME_PATH || findChrome();
if (!chromePath)
  err(
    'Chrome not found. Install Google Chrome from https://www.google.com/chrome/ or pass --chromePath <path>'
  );
if (CHROME_PATH && !existsSync(CHROME_PATH))
  err(`Chrome not found at --chromePath: ${CHROME_PATH}`);

let userDataDir;
let usingIsolatedProfile = false;

mkdirSync(CHROME_STATE_DIR, { recursive: true, mode: 0o700 });

if (REAL_PROFILE) {
  const plan = planRealProfile({
    chromePath,
    profile: PROFILE,
    os: platform(),
    env: process.env,
    major: () => browserMajorVersion(chromePath, platform()),
  });
  if ('error' in plan) err(plan.error);
  userDataDir = plan.userDataDir;
  console.error(
    '[BROWSER] WARNING: Using real user Chrome profile. CDP scripts will have access'
  );
  console.error(
    '[BROWSER] WARNING: to all cookies, auth tokens, and sessions in this profile.'
  );
  console.error(
    '[BROWSER] WARNING: Use --headless for isolated inspection without profile access.'
  );
} else {
  usingIsolatedProfile = true;
  mkdirSync(HEADLESS_PROFILE_DIR, { recursive: true });
  userDataDir = HEADLESS_PROFILE_DIR;
  if (!HEADLESS)
    console.error(
      '[BROWSER] Visible isolated profile (pass --profile <name> for the real profile, after approval)'
    );
}

let chromeArgs: string[];
try {
  chromeArgs = chromeLaunchArgs({
    port: PORT,
    userDataDir,
    isolated: usingIsolatedProfile,
    profile: PROFILE,
    headless: HEADLESS,
    os: platform(),
    userAgent: USER_AGENT,
    windowSize: WINDOW_SIZE,
    enableFeatures: ENABLE_FEATURES,
    disableFeatures: DISABLE_FEATURES,
    blinkFeatures: BLINK_FEATURES,
    chromeFlags: CHROME_FLAGS,
    proxy,
    url: URL_ARG,
    noSandbox: NO_SANDBOX,
  });
} catch (e) {
  if (usingIsolatedProfile)
    rmSync(HEADLESS_PROFILE_DIR, { recursive: true, force: true });
  err(errorMessage(e));
}

const launchedFlags = appliedLaunchFlags({
  chromeFlags: CHROME_FLAGS,
  enableFeatures: ENABLE_FEATURES,
  disableFeatures: DISABLE_FEATURES,
  blinkFeatures: BLINK_FEATURES,
  userAgent: USER_AGENT,
  windowSize: hasFlag('--windowSize') ? WINDOW_SIZE : '',
  proxyRequested,
});

const profileLabel = HEADLESS
  ? 'headless'
  : usingIsolatedProfile
    ? 'isolated-cdp'
    : PROFILE;
console.error(
  `[BROWSER] Launching Chrome: headless=${HEADLESS} port=${PORT} profile=${profileLabel}`
);

// Chrome's start-up complaints (a missing sandbox) go to a file this process can read back.
const launchLog = join(CHROME_STATE_DIR, `cdp-chrome-launch-${PORT}.log`);
const launchLogFd = openSync(launchLog, 'w', 0o600);
const child = spawn(chromePath, chromeArgs, {
  detached: true,
  stdio: ['ignore', 'ignore', launchLogFd],
});
closeSync(launchLogFd);
child.unref();
const launchOutput = () => {
  try {
    return readFileSync(launchLog, 'utf8');
  } catch {
    return '';
  }
};
// A fresh browser starts with this call's origin list and no groups from an earlier browser.
updateSessionMeta(
  PORT,
  {
    allowOrigins,
    groups: undefined,
    contexts: undefined,
    emulation: undefined,
    ...emulationPatch,
  },
  WORKSPACE
);

if (usingIsolatedProfile) writeSession(child.pid);

let attempts = 0;
while (attempts < 40) {
  await new Promise(r => setTimeout(r, 500));
  const info = await checkRunning(PORT);
  if (info) {
    if (usingIsolatedProfile)
      writeSession(child.pid, info.webSocketDebuggerUrl);
    if (URL_ARG && allowOrigins) await enforceLaunchUrlOrigin();
    recordActiveSession(CHROME_STATE_DIR, {
      port: PORT,
      pid: child.pid,
      wsUrl: info.webSocketDebuggerUrl,
      reused: false,
    });
    ok({
      status: 'BROWSER_READY',
      wsUrl: info.webSocketDebuggerUrl,
      port: PORT,
      reused: false,
      browser: info.Browser,
      isolated: usingIsolatedProfile,
      profileDir: usingIsolatedProfile ? HEADLESS_PROFILE_DIR : undefined,
      profileNote: usingIsolatedProfile
        ? isolatedProfileNote(HEADLESS, PORT)
        : undefined,
      sessionFile: usingIsolatedProfile ? SESSION_FILE : null,
      proxyConfigured: proxyRequested,
      enableFeaturesConfigured: launchFeatures(ENABLE_FEATURES),
      windowSize: WINDOW_SIZE || undefined,
      ...launchedFlags,
      ...(allowOrigins ? { allowOrigins } : {}),
      ...emulationReport,
    });
    rmSync(launchLog, { force: true });
    process.exit(0);
  }
  if (isNoSandboxFailure(launchOutput())) break;
  attempts++;
}
const noSandboxFailure = isNoSandboxFailure(launchOutput());
rmSync(launchLog, { force: true });
if (noSandboxFailure) {
  if (child.pid !== undefined)
    try {
      process.kill(child.pid, 'SIGKILL');
    } catch {}
  if (usingIsolatedProfile) {
    rmSync(HEADLESS_PROFILE_DIR, { recursive: true, force: true });
    rmSync(SESSION_FILE, { force: true });
  }
  err(`Chrome exited at start: ${NO_SANDBOX_HINT}`);
}

if (usingIsolatedProfile && existsSync(HEADLESS_PROFILE_DIR)) {
  // The isolated child is ours: stop it before deleting the profile it holds.
  if (child.pid !== undefined) {
    try {
      process.kill(child.pid, 'SIGKILL');
    } catch {}
    await waitForExit(child.pid, 2000);
  }
  rmSync(HEADLESS_PROFILE_DIR, { recursive: true, force: true });
  rmSync(SESSION_FILE, { force: true });
}
if (!usingIsolatedProfile) {
  // The real-profile browser we started never exposed CDP; close it gracefully so the
  // profile is not left held by a window the caller cannot drive.
  try {
    if (child.pid !== undefined) process.kill(child.pid, 'SIGTERM');
  } catch {}
  err(
    `Chrome did not expose CDP on port ${PORT} within 20s for the real profile and was asked to quit. This browser build may ignore --remote-debugging-port on its default data directory; use an isolated session with cookies --from-port or --from-storage-state.`
  );
}
err(
  `Chrome did not respond on port ${PORT} after 20s. Try launching manually with --remote-debugging-port=${PORT}`
);
