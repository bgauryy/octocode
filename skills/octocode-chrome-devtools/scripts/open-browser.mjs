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
const CHROME_PATH  = getArg('--chromePath', '');
const WINDOW_SIZE  = getArg('--windowSize', '');   // e.g. "390x844" for mobile, "1920x1080" for desktop
const USER_AGENT  = getArg('--userAgent', '');

const TMP         = tmpdir();
const SESSION_FILE = join(TMP, `cdp-session-${PORT}.json`);
// Headless Chrome gets its own isolated temp profile — never touches the real user profile
const HEADLESS_PROFILE_DIR = join(TMP, `cdp-chrome-profile-${PORT}`);

function ok(payload)  { console.log(JSON.stringify(payload)); }
function err(message) { console.log(JSON.stringify({ status: 'ERROR', message })); process.exit(1); }

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
  if (!session) { console.error('[BROWSER] No tracked session found for port', PORT); return; }
  if (String(session.port) !== String(PORT)) {
    err(`Tracked session port mismatch: expected ${PORT}, found ${session.port}`);
  }
  if (!processMatchesTrackedSession(session)) {
    console.error(`[BROWSER] Refusing to kill pid=${session.pid}; it does not match the tracked CDP port/profile`);
    try { rmSync(SESSION_FILE, { force: true }); } catch {}
    ok({ status: 'STALE_SESSION_REMOVED', port: PORT });
    return;
  }

  // Kill the process, then wait briefly so Chrome releases profile files.
  try { process.kill(session.pid, 'SIGTERM'); console.error(`[BROWSER] Sent SIGTERM to Chrome pid=${session.pid}`); }
  catch { console.error(`[BROWSER] Process pid=${session.pid} already gone`); }
  const exited = await waitForExit(session.pid);
  if (!exited) {
    try { process.kill(session.pid, 'SIGKILL'); console.error(`[BROWSER] Sent SIGKILL to Chrome pid=${session.pid}`); }
    catch {}
    await waitForExit(session.pid, 1000);
  }

  // Remove temp profile dir
  if (session.profileDir && existsSync(session.profileDir)) {
    try { await removeDirWithRetry(session.profileDir); console.error(`[BROWSER] Removed profile: ${session.profileDir}`); }
    catch (e) { console.error(`[BROWSER] Could not remove profile: ${e.message}`); }
  }

  // Remove session file
  try { rmSync(SESSION_FILE, { force: true }); } catch {}
  console.error('[BROWSER] Session cleaned up');
  ok({ status: 'CLEANED_UP', port: PORT });
}

if (CLEANUP) { await cleanupSession(); process.exit(0); }

const existing = await checkRunning();
if (existing) {
  ok({ status: 'BROWSER_READY', wsUrl: existing.webSocketDebuggerUrl, port: PORT, reused: true, browser: existing.Browser });
  process.exit(0);
}

const chromePath = CHROME_PATH || findChrome();
if (!chromePath) err('Chrome not found. Install Google Chrome from https://www.google.com/chrome/ or pass --chromePath <path>');
if (CHROME_PATH && !existsSync(CHROME_PATH)) err(`Chrome not found at --chromePath: ${CHROME_PATH}`);

const HOME = process.env.HOME ?? process.env.USERPROFILE;

// On macOS, Chrome enforces a single instance per user-data-dir.
// If Chrome is already running without CDP, spawning with the real profile dir
// just hands off to the existing process (which has no CDP port).
// Detect this and fall back to an isolated temp-profile so we force a new process.
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
  // Chrome is already open without CDP — use isolated temp profile to force a new CDP process
  usingIsolatedProfile = true;
  mkdirSync(HEADLESS_PROFILE_DIR, { recursive: true });
  userDataDir = HEADLESS_PROFILE_DIR;
  console.error('[BROWSER] Chrome already running without CDP — launching isolated CDP session');
} else {
  userDataDir = platform() === 'darwin'
    ? `${HOME}/Library/Application Support/Google/Chrome`
    : platform() === 'win32'
      ? `${process.env.LOCALAPPDATA}\\Google\\Chrome\\User Data`
      : `${HOME}/.config/google-chrome`;
  // SECURITY WARNING: real user profile exposes all cookies, stored credentials, and
  // active authenticated sessions to CDP scripts. Only do this when explicitly needed
  // for auth-dependent tasks and you trust the scripts being run.
  console.error('[BROWSER] WARNING: Using real user Chrome profile. CDP scripts will have access');
  console.error('[BROWSER] WARNING: to all cookies, auth tokens, and sessions in this profile.');
  console.error('[BROWSER] WARNING: Use --headless for isolated inspection without auth exposure.');
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
// --no-sandbox removes Chrome's OS process sandbox. Only add it on Linux where it is
// required (e.g. running as root or inside Docker). Never add it on macOS or Windows.
if (HEADLESS && platform() === 'linux') chromeArgs.push('--no-sandbox', '--disable-setuid-sandbox');
if (USER_AGENT) chromeArgs.push(`--user-agent=${USER_AGENT}`);
if (WINDOW_SIZE) {
  // Accept "WxH" or "W,H" — normalise to Chrome's expected "W,H" format
  chromeArgs.push(`--window-size=${WINDOW_SIZE.replace('x', ',')}`);
}
if (URL_ARG)   chromeArgs.push(URL_ARG);

const profileLabel = HEADLESS ? 'headless' : usingIsolatedProfile ? 'isolated-cdp' : PROFILE;
console.error(`[BROWSER] Launching Chrome: headless=${HEADLESS} port=${PORT} profile=${profileLabel}`);

const child = spawn(chromePath, chromeArgs, { detached: true, stdio: 'ignore' });
child.unref();

// Track session for later cleanup (headless or isolated non-headless)
if (HEADLESS || usingIsolatedProfile) writeSession(child.pid);

// Poll until ready (max 20s)
let attempts = 0;
while (attempts < 40) {
  await new Promise(r => setTimeout(r, 500));
  const info = await checkRunning();
  if (info) {
    ok({ status: 'BROWSER_READY', wsUrl: info.webSocketDebuggerUrl, port: PORT, reused: false, browser: info.Browser, isolated: HEADLESS || usingIsolatedProfile, sessionFile: (HEADLESS || usingIsolatedProfile) ? SESSION_FILE : null });
    process.exit(0);
  }
  attempts++;
}

// If we get here Chrome failed to start — clean up the temp profile
if ((HEADLESS || usingIsolatedProfile) && existsSync(HEADLESS_PROFILE_DIR)) {
  rmSync(HEADLESS_PROFILE_DIR, { recursive: true, force: true });
  rmSync(SESSION_FILE, { force: true });
}
err(`Chrome did not respond on port ${PORT} after 20s. Try launching manually with --remote-debugging-port=${PORT}`);
