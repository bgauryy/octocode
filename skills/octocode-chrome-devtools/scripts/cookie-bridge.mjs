#!/usr/bin/env node
/**
 * Opt-in cookie transfer into an isolated CDP session.
 * Never prints cookie values. Requires --i-understand-secrets.
 *
 * Sources (pick one):
 *   --from-port <n>              pull via Network.getAllCookies / getCookies
 *   --from-profile <name>        briefly launch real profile on --source-port
 *   --from-storage-state <path>  Playwright-style cookies JSON
 *
 * Target:
 *   --to-port <n>                inject via Network.setCookies
 *
 * Optional:
 *   --urls "https://a.com,https://b.com"   filter / setCookies URL hints
 *   --export-storage-state <path>          write jar (0600) without logging values
 *   --source-port 9333                     profile launch port (default 9333)
 *   --dry-run                              count/names only; no inject
 */
import { spawn } from 'child_process';
import {
  existsSync, mkdirSync, readFileSync, writeFileSync,
} from 'fs';
import { dirname, join, resolve } from 'path';
import { fileURLToPath } from 'url';
import { platform } from 'os';

const __dir = dirname(fileURLToPath(import.meta.url));
const argv = process.argv.slice(2);
const getArg = (flag, def) => {
  const i = argv.indexOf(flag);
  return i !== -1 && argv[i + 1] ? argv[i + 1] : def;
};
const hasFlag = (flag) => argv.includes(flag);

if (hasFlag('--help') || argv.length === 0) {
  console.error(`Usage: node cookie-bridge.mjs --i-understand-secrets --to-port 9222 \\
  (--from-port 9333 | --from-profile Default | --from-storage-state state.json) \\
  [--urls "https://example.com"] [--export-storage-state out.json] [--dry-run]

Gate: --i-understand-secrets is required. Values are never printed.
Prefer --from-port (existing CDP) or --from-storage-state over --from-profile when Chrome is already open.`);
  process.exit(argv.length === 0 ? 1 : 0);
}

const ACK = hasFlag('--i-understand-secrets');
const TO_PORT = getArg('--to-port', '');
const FROM_PORT = getArg('--from-port', '');
const FROM_PROFILE = getArg('--from-profile', '');
const FROM_STATE = getArg('--from-storage-state', '');
const EXPORT_STATE = getArg('--export-storage-state', '');
const SOURCE_PORT = getArg('--source-port', '9333');
const URLS = (getArg('--urls', '') || '').split(',').map(s => s.trim()).filter(Boolean);
const DRY_RUN = hasFlag('--dry-run');

function fail(msg) {
  console.error(`[COOKIE_BRIDGE] ${msg}`);
  process.exit(1);
}

if (!ACK) fail('Refusing: pass --i-understand-secrets after user approval (CDP can read session secrets).');
if (!TO_PORT && !EXPORT_STATE) fail('Need --to-port and/or --export-storage-state.');
const sources = [FROM_PORT, FROM_PROFILE, FROM_STATE].filter(Boolean);
if (sources.length !== 1) fail('Pick exactly one of --from-port, --from-profile, --from-storage-state.');

const WS = globalThis.WebSocket;
const [nodeMajor] = process.versions.node.split('.').map(Number);
if (!Number.isFinite(nodeMajor) || nodeMajor < 22 || !WS) {
  fail(`Node.js 22+ required (you have ${process.versions.node}). Native WebSocket is unavailable.`);
}

function cookieMeta(c) {
  return {
    name: c.name,
    domain: c.domain || '',
    path: c.path || '/',
    secure: Boolean(c.secure),
    httpOnly: Boolean(c.httpOnly),
    sameSite: c.sameSite || 'Lax',
    session: c.session ?? (c.expires === -1 || c.expires === undefined),
  };
}

function toSetCookie(c) {
  const out = {
    name: c.name,
    value: c.value,
    domain: c.domain,
    path: c.path || '/',
    secure: Boolean(c.secure),
    httpOnly: Boolean(c.httpOnly),
  };
  if (c.expires != null && c.expires !== -1) out.expires = c.expires;
  if (c.sameSite) out.sameSite = c.sameSite;
  if (c.url) out.url = c.url;
  else if (URLS[0] && !c.domain) out.url = URLS[0];
  return out;
}

function toStorageStateCookie(c) {
  return {
    name: c.name,
    value: c.value,
    domain: c.domain || '',
    path: c.path || '/',
    expires: c.expires ?? -1,
    httpOnly: Boolean(c.httpOnly),
    secure: Boolean(c.secure),
    sameSite: c.sameSite === 'None' || c.sameSite === 'Strict' || c.sameSite === 'Lax'
      ? c.sameSite
      : 'Lax',
  };
}

function filterCookies(cookies) {
  if (!URLS.length) return cookies;
  return cookies.filter((c) => {
    const domain = String(c.domain || '').replace(/^\./, '');
    return URLS.some((u) => {
      try {
        const host = new URL(u).hostname;
        return host === domain || host.endsWith(`.${domain}`) || domain.endsWith(host);
      } catch {
        return false;
      }
    });
  });
}

async function cdpHttp(port, path, method = 'GET') {
  const res = await fetch(`http://127.0.0.1:${port}${path}`, {
    method,
    signal: AbortSignal.timeout(5000),
  });
  if (!res.ok) throw new Error(`CDP HTTP ${res.status} ${path}`);
  return res.json();
}

async function connectPort(port) {
  const version = await cdpHttp(port, '/json/version');
  if (!version.webSocketDebuggerUrl) throw new Error(`No browser WS on port ${port}`);
  return createBrowserSession(version.webSocketDebuggerUrl);
}

/** Network cookies need a page/target session, not the root browser WS. */
async function attachPageSession(port) {
  const browser = await connectPort(port);
  let targets;
  try {
    const listed = await browser.send('Target.getTargets');
    targets = listed.targetInfos || [];
  } catch {
    targets = await cdpHttp(port, '/json');
  }
  let page = (targets || []).find(t => (t.type === 'page' || t.type === 'tab') && !String(t.url || '').startsWith('devtools://'));
  if (!page) {
    await browser.send('Target.createTarget', { url: 'about:blank' });
    const listed = await browser.send('Target.getTargets');
    targets = listed.targetInfos || [];
    page = (targets || []).find(t => t.type === 'page' || t.type === 'tab');
  }
  if (!page) {
    browser.close();
    throw new Error(`No page target on port ${port}`);
  }
  const targetId = page.targetId || page.id;
  const attached = await browser.send('Target.attachToTarget', { targetId, flatten: true });
  const sessionId = attached.sessionId;
  return {
    sessionId,
    async send(method, params = {}) {
      return browser.send(method, params, sessionId);
    },
    close() { browser.close(); },
  };
}

function createBrowserSession(wsUrl) {
  return new Promise((resolveSession, rejectSession) => {
    const ws = new WS(wsUrl);
    let msgId = 1;
    const pending = new Map();
    let closed = false;

    ws.onopen = () => {
      resolveSession({
        send(method, params = {}, sessionId) {
          if (closed) return Promise.reject(new Error('session closed'));
          return new Promise((res, rej) => {
            const id = msgId++;
            const timer = setTimeout(() => {
              pending.delete(id);
              rej(new Error(`CDP timeout for ${method}`));
            }, 30000);
            pending.set(id, { res, rej, timer });
            const payload = { id, method, params };
            if (sessionId) payload.sessionId = sessionId;
            ws.send(JSON.stringify(payload));
          });
        },
        close() {
          closed = true;
          try { ws.close(); } catch { /* ignore */ }
        },
      });
    };
    ws.onmessage = (ev) => {
      let msg;
      try { msg = JSON.parse(ev.data); } catch { return; }
      if (msg.id == null) return;
      const p = pending.get(msg.id);
      if (!p) return;
      clearTimeout(p.timer);
      pending.delete(msg.id);
      if (msg.error) p.rej(new Error(msg.error.message || JSON.stringify(msg.error)));
      else p.res(msg.result);
    };
    ws.onerror = () => rejectSession(new Error('WebSocket error'));
  });
}

async function readCookiesFromPort(port) {
  const page = await attachPageSession(port);
  try {
    await page.send('Network.enable');
    let cookies = [];
    if (URLS.length) {
      const res = await page.send('Network.getCookies', { urls: URLS });
      cookies = res.cookies || [];
    } else {
      const res = await page.send('Network.getAllCookies');
      cookies = res.cookies || [];
    }
    return cookies;
  } finally {
    page.close();
  }
}

async function writeCookiesToPort(port, cookies) {
  const page = await attachPageSession(port);
  try {
    await page.send('Network.enable');
    const payload = cookies.map(toSetCookie);
    await page.send('Network.setCookies', { cookies: payload });
    return payload.length;
  } finally {
    page.close();
  }
}

function readStorageState(path) {
  const raw = JSON.parse(readFileSync(path, 'utf8'));
  const list = Array.isArray(raw) ? raw : (raw.cookies || []);
  if (!Array.isArray(list)) fail('storage state must be {cookies:[...]} or an array');
  return list;
}

function writeStorageState(path, cookies) {
  mkdirSync(dirname(resolve(path)), { recursive: true, mode: 0o700 });
  writeFileSync(
    path,
    `${JSON.stringify({ cookies: cookies.map(toStorageStateCookie) }, null, 2)}\n`,
    { mode: 0o600 },
  );
}

function launchProfileSource(profile, port) {
  return new Promise((resolveLaunch, rejectLaunch) => {
    const openBrowser = join(__dir, 'open-browser.mjs');
    const args = [openBrowser, '--profile', profile, '--port', String(port), '--url', 'about:blank'];
    console.error(`[COOKIE_BRIDGE] Launching profile=${profile} on port=${port} (Chrome must not hold this profile lock)`);
    const child = spawn(process.execPath, args, { stdio: ['ignore', 'pipe', 'pipe'] });
    let out = '';
    child.stdout.on('data', (d) => { out += d; });
    child.stderr.on('data', (d) => { out += d; });
    child.on('exit', (code) => {
      if (code !== 0) {
        rejectLaunch(new Error(`open-browser failed (exit ${code}): ${out.slice(-500)}`));
        return;
      }
      resolveLaunch(out);
    });
  });
}

async function cleanupSourcePort(port) {
  const openBrowser = join(__dir, 'open-browser.mjs');
  await new Promise((resolveCleanup) => {
    const child = spawn(process.execPath, [openBrowser, '--port', String(port), '--cleanup'], {
      stdio: 'ignore',
    });
    child.on('exit', () => resolveCleanup());
  });
}

async function main() {
  console.log('[ACTION] cookie-bridge start (values redacted)');
  if (platform() === 'darwin' && FROM_PROFILE) {
    console.log('[FINDING] macOS: quit Chrome fully before --from-profile, or use --from-port / --from-storage-state');
  }

  let launchedSource = false;
  let cookies = [];

  try {
    if (FROM_STATE) {
      if (!existsSync(FROM_STATE)) fail(`storage state not found: ${FROM_STATE}`);
      cookies = readStorageState(FROM_STATE);
      console.log(`[METRIC] loaded storage-state path=${FROM_STATE}`);
    } else if (FROM_PORT) {
      cookies = await readCookiesFromPort(FROM_PORT);
      console.log(`[METRIC] loaded from-port=${FROM_PORT}`);
    } else if (FROM_PROFILE) {
      await launchProfileSource(FROM_PROFILE, SOURCE_PORT);
      launchedSource = true;
      cookies = await readCookiesFromPort(SOURCE_PORT);
      console.log(`[METRIC] loaded from-profile=${FROM_PROFILE} source-port=${SOURCE_PORT}`);
    }

    cookies = filterCookies(cookies);
    const metas = cookies.map(cookieMeta);
    console.log(`[METRIC] cookies=${metas.length} domains=${new Set(metas.map(m => m.domain)).size}`);
    console.log(`[FINDING] cookie-names=${metas.map(m => m.name).slice(0, 40).join(',')}${metas.length > 40 ? ',…' : ''}`);

    if (EXPORT_STATE) {
      writeStorageState(EXPORT_STATE, cookies);
      console.log(`[ARTIFACT] storage-state ${resolve(EXPORT_STATE)} mode=0600`);
    }

    if (DRY_RUN) {
      console.log('[REASON] dry-run: no inject');
      return;
    }

    if (TO_PORT) {
      const n = await writeCookiesToPort(TO_PORT, cookies);
      console.log(`[AUTH_COMPLETE] injected=${n} to-port=${TO_PORT}`);
      console.log('[REASON] navigate/target app on to-port; do not print cookie values');
    }
  } finally {
    if (launchedSource) {
      await cleanupSourcePort(SOURCE_PORT);
      console.log(`[ACTION] cleaned source-port=${SOURCE_PORT}`);
    }
  }
}

main().catch((err) => {
  console.error(`[EXCEPTION] ${err.message}`);
  process.exit(1);
});
