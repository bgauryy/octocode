#!/usr/bin/env node

import { spawn } from 'child_process';
import { platform } from 'os';
import { existsSync, writeFileSync, rmSync, mkdirSync } from 'fs';
import { join } from 'path';
import { propagateOctocodeEnv } from './octocode-config.mjs';
import { browserMajorVersion, planRealProfile } from './real-profile.mjs';
import { flagArgs } from './cli-flags.mjs';
import { errorMessage } from './error-message.mjs';
import {
  findChrome,
  removeDirWithRetry,
  waitForExit,
} from './browser-process.mjs';
import {
  STATE_SUBPATH,
  checkRunning,
  discoverListenerSession,
  processMatchesTrackedSession,
  profileDirIn,
  readSession,
  sessionFileIn,
} from './browser-session.mjs';
import {
  chromeLaunchArgs,
  effectiveProxy,
  loadProxyConfig,
} from './browser-launch-config.mjs';

const argv = process.argv.slice(2);
const { getArg, hasFlag } = flagArgs(argv);
// getArg returns the default whenever the flag has no value, so these are never undefined.

if (hasFlag('--help') || hasFlag('-h')) {
  console.log(`Usage: open-browser.mjs [options]

Launch or reuse a Chrome DevTools Protocol session.

Options:
  --port <n>              CDP port (default: 9222)
  --url <url>             Initial URL
  --headless              Use an isolated headless profile
  --profile <name>        Use the real browser profile (needs approval; quit the browser;
                          Google Chrome 136+ refuses, Chromium/Chrome for Testing work)
  --chromePath <path>     Explicit Chrome executable
  --windowSize <WxH>      Browser window size
  --enableFeatures <csv>  Chrome feature flags
  --userAgent <ua>        User-Agent override
  --proxyServer <url>     Proxy (also --proxyBypassList, --proxyPacUrl)
  --config <path>         Proxy configuration file
  --cleanup               Stop the isolated session tracked for this port (from any cwd)
  --dry-run               Preview cleanup without stopping Chrome
  -h, --help              Show this help and exit`);
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
const USER_AGENT = getArg('--userAgent', '');
const PROXY_SERVER = getArg('--proxyServer', '');
const PROXY_BYPASS_LIST = getArg('--proxyBypassList', '');
const PROXY_PAC_URL = getArg('--proxyPacUrl', '');
const CONFIG_PATH = getArg('--config', '');

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
const CHROME_STATE_DIR = join(WORKSPACE, ...STATE_SUBPATH);
const SESSION_FILE = sessionFileIn(CHROME_STATE_DIR, PORT);
const HEADLESS_PROFILE_DIR = profileDirIn(CHROME_STATE_DIR, PORT);

function writeSession(pid: number | undefined) {
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
    }),
    { mode: 0o600 }
  );
}

async function cleanupSession() {
  let session = readSession(SESSION_FILE);
  let sessionFile = SESSION_FILE;
  let owner: Record<string, unknown> = { workspace: WORKSPACE, sessionFile };
  if (!session) {
    const found = await discoverListenerSession(null, PORT);
    if (found.session === undefined) {
      console.error(
        `[BROWSER] No tracked session found for port ${PORT} (${found.reason})`
      );
      ok({
        status: 'NO_TRACKED_SESSION',
        port: PORT,
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
    ok({ status: 'STALE_SESSION_REMOVED', port: PORT, ...owner });
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
  console.error('[BROWSER] Session cleaned up');
  ok({ status: 'CLEANED_UP', port: PORT, pid: session.pid, ...owner });
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

const existing = await checkRunning(PORT);
if (existing) {
  // Report which workspace's isolated session owns the reused browser, if any.
  const owner = await discoverListenerSession(existing, PORT);
  const notApplied = [
    proxyRequested && 'proxy settings',
    ENABLE_FEATURES && '--enableFeatures',
    REAL_PROFILE && '--profile',
    USER_AGENT && '--userAgent',
    hasFlag('--windowSize') && '--windowSize',
  ].filter(Boolean);
  ok({
    status: 'BROWSER_READY',
    wsUrl: existing.webSocketDebuggerUrl,
    port: PORT,
    reused: true,
    browser: existing.Browser,
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

const chromeArgs = chromeLaunchArgs({
  port: PORT,
  userDataDir,
  isolated: usingIsolatedProfile,
  profile: PROFILE,
  headless: HEADLESS,
  os: platform(),
  userAgent: USER_AGENT,
  windowSize: WINDOW_SIZE,
  enableFeatures: ENABLE_FEATURES,
  proxy,
  url: URL_ARG,
});

const profileLabel = HEADLESS
  ? 'headless'
  : usingIsolatedProfile
    ? 'isolated-cdp'
    : PROFILE;
console.error(
  `[BROWSER] Launching Chrome: headless=${HEADLESS} port=${PORT} profile=${profileLabel}`
);

const child = spawn(chromePath, chromeArgs, {
  detached: true,
  stdio: 'ignore',
});
child.unref();

if (usingIsolatedProfile) writeSession(child.pid);

let attempts = 0;
while (attempts < 40) {
  await new Promise(r => setTimeout(r, 500));
  const info = await checkRunning(PORT);
  if (info) {
    ok({
      status: 'BROWSER_READY',
      wsUrl: info.webSocketDebuggerUrl,
      port: PORT,
      reused: false,
      browser: info.Browser,
      isolated: usingIsolatedProfile,
      sessionFile: usingIsolatedProfile ? SESSION_FILE : null,
      proxyConfigured: proxyRequested,
      enableFeaturesConfigured: ENABLE_FEATURES || undefined,
      windowSize: WINDOW_SIZE || undefined,
    });
    process.exit(0);
  }
  attempts++;
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
