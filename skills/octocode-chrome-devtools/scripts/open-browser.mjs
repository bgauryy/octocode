#!/usr/bin/env node
// Launch Chrome with CDP enabled; tracks isolated sessions for cleanup.

import { spawn, execSync, execFileSync } from 'child_process';
import { platform, tmpdir }  from 'os';
import { existsSync, writeFileSync, readFileSync, rmSync, mkdirSync } from 'fs';
import { join } from 'path';

const argv = process.argv.slice(2);
const getArg  = (flag, def) => { const i = argv.indexOf(flag); return i !== -1 && argv[i + 1] ? argv[i + 1] : def; };
const hasFlag = (flag) => argv.includes(flag);

const PORT        = getArg('--port', '9222');
const PROFILE     = getArg('--profile', 'Default');
const URL_ARG     = getArg('--url', '');
const HEADLESS    = hasFlag('--headless');
const CLEANUP     = hasFlag('--cleanup');
const DRY_RUN     = hasFlag('--dry-run');
const CHROME_PATH  = getArg('--chromePath', '');
const WINDOW_SIZE  = getArg('--windowSize', '');
const USER_AGENT  = getArg('--userAgent', '');
const PROXY_SERVER = getArg('--proxyServer', '');
const PROXY_BYPASS_LIST = getArg('--proxyBypassList', '');
const PROXY_PAC_URL = getArg('--proxyPacUrl', '');
const CONFIG_PATH = getArg('--config', '');

const TMP         = tmpdir();
const SESSION_FILE = join(TMP, `cdp-session-${PORT}.json`);
// Headless always uses an isolated temp profile.
const HEADLESS_PROFILE_DIR = join(TMP, `cdp-chrome-profile-${PORT}`);

function ok(payload)  { console.log(JSON.stringify(payload)); }
function err(message) { console.log(JSON.stringify({ status: 'ERROR', message })); process.exit(1); }

function readJsonFile(filePath, { strict = false } = {}) {
  if (!filePath || !existsSync(filePath)) return null;
  try {
    return JSON.parse(readFileSync(filePath, 'utf8'));
  } catch (jsonErr) {
    if (strict) err(`Invalid JSON in config file: ${filePath} (${jsonErr.message})`);
    console.error(`[BROWSER] Warning: ignoring invalid JSON config ${filePath}: ${jsonErr.message}`);
    return null;
  }
}

function normalizeProxyConfig(raw) {
  if (!raw || typeof raw !== 'object') return null;
  const enabled = raw.enabled;
  const server = typeof raw.server === 'string' ? raw.server.trim() : '';
  const bypassList = typeof raw.bypassList === 'string' ? raw.bypassList.trim() : '';
  const pacUrl = typeof raw.pacUrl === 'string' ? raw.pacUrl.trim() : '';
  return {
    enabled: enabled === undefined ? true : Boolean(enabled),
    server,
    bypassList,
    pacUrl,
  };
}

function loadProxyConfig() {
  const cwdConfig = join(process.cwd(), '.octocode', 'chrome-devtools.json');
  const homeConfig = join(process.env.HOME ?? process.env.USERPROFILE ?? '', '.octocode', 'config.json');
  if (CONFIG_PATH && !existsSync(CONFIG_PATH)) err(`Config file not found: ${CONFIG_PATH}`);
  const candidateFiles = CONFIG_PATH ? [CONFIG_PATH] : [cwdConfig, homeConfig];

  for (const configFile of candidateFiles) {
    const json = readJsonFile(configFile, { strict: Boolean(CONFIG_PATH) });
    if (!json || typeof json !== 'object') continue;

    const candidates = [
      json?.proxy,
      json?.chromeDevtools?.proxy,
      json?.skills?.['octocode-chrome-devtools']?.proxy,
    ];
    for (const candidate of candidates) {
      const normalized = normalizeProxyConfig(candidate);
      if (normalized) return normalized;
    }
  }
  return null;
}

function findChrome() {
  if (platform() === 'darwin') {
    const candidates = [
      '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
      '/Applications/Chromium.app/Contents/MacOS/Chromium',
      '/Applications/Google Chrome Canary.app/Contents/MacOS/Google Chrome Canary',
      '/Applications/Brave Browser.app/Contents/MacOS/Brave Browser',
    ];
    return candidates.find(p => existsSync(p)) ?? null;
  }
  if (platform() === 'linux') {
    for (const bin of ['google-chrome', 'google-chrome-stable', 'chromium-browser', 'chromium']) {
      try { execSync(`which ${bin}`, { stdio: 'ignore' }); return bin; } catch {}
    }
  }
  if (platform() === 'win32') {
    const candidates = [
      `${process.env.LOCALAPPDATA}\\Google\\Chrome\\Application\\chrome.exe`,
      `${process.env.PROGRAMFILES}\\Google\\Chrome\\Application\\chrome.exe`,
      `${process.env['PROGRAMFILES(X86)']}\\Google\\Chrome\\Application\\chrome.exe`,
      `${process.env.LOCALAPPDATA}\\Chromium\\Application\\chrome.exe`,
    ];
    return candidates.find(p => p && existsSync(p)) ?? null;
  }
  return null;
}

async function checkRunning() {
  try {
    const res = await fetch(`http://localhost:${PORT}/json/version`, { signal: AbortSignal.timeout(2000) });
    if (res.ok) return await res.json();
  } catch {}
  return null;
}

function readSession() {
  try { return JSON.parse(readFileSync(SESSION_FILE, 'utf8')); } catch { return null; }
}

function writeSession(pid) {
  writeFileSync(SESSION_FILE, JSON.stringify({
    pid,
    port: PORT,
    profileDir: HEADLESS_PROFILE_DIR,
    headless: HEADLESS,
    isolated: HEADLESS || usingIsolatedProfile,
    startedAt: Date.now(),
    chromePath,
  }), { mode: 0o600 });
}

function getProcessCommand(pid) {
  const pidText = String(pid);
  if (!/^\d+$/.test(pidText)) return '';
  try {
    if (platform() === 'darwin' || platform() === 'linux') {
      return execFileSync('ps', ['-p', pidText, '-o', 'command='], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'] }).trim();
    }
    if (platform() === 'win32') {
      return execFileSync('powershell.exe', [
        '-NoProfile',
        '-Command',
        `(Get-CimInstance Win32_Process -Filter "ProcessId=${pidText}").CommandLine`,
      ], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'] }).trim();
    }
  } catch {}
  return '';
}

function processMatchesTrackedSession(session) {
  const command = getProcessCommand(session.pid);
  if (!command) return false;

  const isChrome = /Chrome|Chromium|Brave Browser|chrome|chromium|chrome\.exe/i.test(command);
  const hasPort = command.includes(`--remote-debugging-port=${session.port}`);
  const hasProfile = session.profileDir && command.includes(session.profileDir);

  return isChrome && hasPort && hasProfile;
}

async function waitForExit(pid, timeoutMs = 3000) {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    try {
      process.kill(pid, 0);
      await new Promise(r => setTimeout(r, 100));
    } catch {
      return true;
    }
  }
  return false;
}

async function removeDirWithRetry(dir) {
  for (let attempt = 0; attempt < 5; attempt++) {
    try {
      rmSync(dir, { recursive: true, force: true });
      if (!existsSync(dir)) return true;
    } catch (e) {
      if (attempt === 4) throw e;
    }
    await new Promise(r => setTimeout(r, 200));
  }
  return !existsSync(dir);
}

async function cleanupSession() {
  const session = readSession();
  if (!session) {
    console.error('[BROWSER] No tracked session found for port', PORT);
    ok({ status: 'NO_TRACKED_SESSION', port: PORT });
    return;
  }
  if (String(session.port) !== String(PORT)) {
    err(`Tracked session port mismatch: expected ${PORT}, found ${session.port}`);
  }
  if (!processMatchesTrackedSession(session)) {
    console.error(`[BROWSER] Refusing to kill pid=${session.pid}; it does not match the tracked CDP port/profile`);
    if (DRY_RUN) {
      ok({ status: 'CLEANUP_DRY_RUN', port: PORT, pid: session.pid, matchesTrackedSession: false, wouldKill: false });
      return;
    }
    try { rmSync(SESSION_FILE, { force: true }); } catch {}
    ok({ status: 'STALE_SESSION_REMOVED', port: PORT });
    return;
  }

  if (DRY_RUN) {
    ok({ status: 'CLEANUP_DRY_RUN', port: PORT, pid: session.pid, matchesTrackedSession: true, wouldKill: true, profileDir: session.profileDir });
    return;
  }

  try { process.kill(session.pid, 'SIGTERM'); console.error(`[BROWSER] Sent SIGTERM to Chrome pid=${session.pid}`); }
  catch { console.error(`[BROWSER] Process pid=${session.pid} already gone`); }
  const exited = await waitForExit(session.pid);
  if (!exited) {
    try { process.kill(session.pid, 'SIGKILL'); console.error(`[BROWSER] Sent SIGKILL to Chrome pid=${session.pid}`); }
    catch {}
    await waitForExit(session.pid, 1000);
  }

  if (session.profileDir && existsSync(session.profileDir)) {
    try { await removeDirWithRetry(session.profileDir); console.error(`[BROWSER] Removed profile: ${session.profileDir}`); }
    catch (e) { console.error(`[BROWSER] Could not remove profile: ${e.message}`); }
  }

  try { rmSync(SESSION_FILE, { force: true }); } catch {}
  console.error('[BROWSER] Session cleaned up');
  ok({ status: 'CLEANED_UP', port: PORT });
}

if (CLEANUP) { await cleanupSession(); process.exit(0); }

const proxyConfig = loadProxyConfig();
const effectiveProxyServer =
  PROXY_SERVER || (proxyConfig?.enabled ? proxyConfig.server : '');
const effectiveProxyBypassList =
  PROXY_BYPASS_LIST || (proxyConfig?.enabled ? proxyConfig.bypassList : '');
const effectiveProxyPacUrl =
  PROXY_PAC_URL || (proxyConfig?.enabled ? proxyConfig.pacUrl : '');
const proxyRequested = Boolean(effectiveProxyServer || effectiveProxyPacUrl);

const existing = await checkRunning();
if (existing) {
  ok({
    status: 'BROWSER_READY',
    wsUrl: existing.webSocketDebuggerUrl,
    port: PORT,
    reused: true,
    browser: existing.Browser,
    proxyConfigured: false,
    proxyRequested,
    warning: proxyRequested
      ? 'Existing Chrome was reused; proxy settings cannot be applied to an already-running CDP session. Run cleanup or use another port for a fresh proxied session.'
      : undefined,
  });
  process.exit(0);
}

const chromePath = CHROME_PATH || findChrome();
if (!chromePath) err('Chrome not found. Install Google Chrome from https://www.google.com/chrome/ or pass --chromePath <path>');
if (CHROME_PATH && !existsSync(CHROME_PATH)) err(`Chrome not found at --chromePath: ${CHROME_PATH}`);

const HOME = process.env.HOME ?? process.env.USERPROFILE;

// If Chrome is already running without CDP, real-profile launches may hand off to it.
function isChromeRunning() {
  if (platform() === 'darwin') {
    try { execSync('pgrep -x "Google Chrome" > /dev/null 2>&1'); return true; } catch { return false; }
  }
  if (platform() === 'linux') {
    for (const name of ['chrome', 'google-chrome', 'chromium', 'chromium-browser']) {
      try { execSync(`pgrep -x "${name}" > /dev/null 2>&1`); return true; } catch {}
    }
    return false;
  }
  if (platform() === 'win32') {
    try {
      const out = execSync('tasklist /FI "IMAGENAME eq chrome.exe" /NH', { encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'] });
      return out.includes('chrome.exe');
    } catch { return false; }
  }
  return false;
}

let userDataDir;
let usingIsolatedProfile = false;

if (HEADLESS) {
  mkdirSync(HEADLESS_PROFILE_DIR, { recursive: true });
  userDataDir = HEADLESS_PROFILE_DIR;
} else if (isChromeRunning()) {
  usingIsolatedProfile = true;
  mkdirSync(HEADLESS_PROFILE_DIR, { recursive: true });
  userDataDir = HEADLESS_PROFILE_DIR;
  console.error('[BROWSER] Chrome already running without CDP - launching isolated CDP session');
} else {
  userDataDir = platform() === 'darwin'
    ? `${HOME}/Library/Application Support/Google/Chrome`
    : platform() === 'win32'
      ? `${process.env.LOCALAPPDATA}\\Google\\Chrome\\User Data`
      : `${HOME}/.config/google-chrome`;
  console.error('[BROWSER] WARNING: Using real user Chrome profile. CDP scripts will have access');
  console.error('[BROWSER] WARNING: to all cookies, auth tokens, and sessions in this profile.');
  console.error('[BROWSER] WARNING: Use --headless for isolated inspection without profile access.');
}

const chromeArgs = [
  `--remote-debugging-port=${PORT}`,
  `--user-data-dir=${userDataDir}`,
  '--no-first-run',
  '--no-default-browser-check',
  '--disable-background-mode',
];

if (!HEADLESS && !usingIsolatedProfile) chromeArgs.push(`--profile-directory=${PROFILE}`, '--restore-last-session');
if (HEADLESS)  chromeArgs.push('--headless=new', '--disable-gpu', '--disable-dev-shm-usage');
if (HEADLESS && platform() === 'linux') chromeArgs.push('--no-sandbox', '--disable-setuid-sandbox');
if (USER_AGENT) chromeArgs.push(`--user-agent=${USER_AGENT}`);
if (WINDOW_SIZE) {
  chromeArgs.push(`--window-size=${WINDOW_SIZE.replace('x', ',')}`);
}
if (effectiveProxyServer) chromeArgs.push(`--proxy-server=${effectiveProxyServer}`);
if (effectiveProxyBypassList) chromeArgs.push(`--proxy-bypass-list=${effectiveProxyBypassList}`);
if (effectiveProxyPacUrl && !effectiveProxyServer) chromeArgs.push(`--proxy-pac-url=${effectiveProxyPacUrl}`);
if (URL_ARG)   chromeArgs.push(URL_ARG);

const profileLabel = HEADLESS ? 'headless' : usingIsolatedProfile ? 'isolated-cdp' : PROFILE;
console.error(`[BROWSER] Launching Chrome: headless=${HEADLESS} port=${PORT} profile=${profileLabel}`);

const child = spawn(chromePath, chromeArgs, { detached: true, stdio: 'ignore' });
child.unref();

if (HEADLESS || usingIsolatedProfile) writeSession(child.pid);

let attempts = 0;
while (attempts < 40) {
  await new Promise(r => setTimeout(r, 500));
  const info = await checkRunning();
  if (info) {
    ok({
      status: 'BROWSER_READY',
      wsUrl: info.webSocketDebuggerUrl,
      port: PORT,
      reused: false,
      browser: info.Browser,
      isolated: HEADLESS || usingIsolatedProfile,
      sessionFile: (HEADLESS || usingIsolatedProfile) ? SESSION_FILE : null,
      proxyConfigured: proxyRequested,
    });
    process.exit(0);
  }
  attempts++;
}

if ((HEADLESS || usingIsolatedProfile) && existsSync(HEADLESS_PROFILE_DIR)) {
  rmSync(HEADLESS_PROFILE_DIR, { recursive: true, force: true });
  rmSync(SESSION_FILE, { force: true });
}
err(`Chrome did not respond on port ${PORT} after 20s. Try launching manually with --remote-debugging-port=${PORT}`);
