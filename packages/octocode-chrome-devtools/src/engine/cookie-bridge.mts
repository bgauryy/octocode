#!/usr/bin/env node
import { connectCDP } from './cdp-connection.mjs';
/**
 * Opt-in cookie transfer into an isolated CDP session.
 * Never prints cookie values. Requires --i-understand-secrets.
 *
 * Sources (pick one):
 *   --from-port <n>              pull via Storage.getCookies
 *   --from-profile <name>        briefly launch real profile on --source-port
 *   --from-storage-state <path>  Playwright-style cookies JSON
 *
 * Target:
 *   --to-port <n>                inject via Storage.setCookies, then read back
 *
 * Optional:
 *   --urls "https://a.com,https://b.com"   keep cookies that domain-match these hosts
 *   --export-storage-state <path>          write jar (0600) without logging values
 *   --source-port 9333                     profile launch port (default 9333)
 *   --chromePath <path>                    browser that owns --from-profile (Chromium, Chrome for Testing)
 *   --dry-run                              count/names only; no inject
 */
import { spawn } from 'child_process';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'fs';
import { dirname, isAbsolute, join, relative, resolve } from 'path';
import {
  cookieKey,
  cookieMeta,
  filterCookies,
  readStorageStateCookies,
  toSetCookie,
  toStorageStateCookie,
  type CookieRecord,
} from './cookie-jar.mjs';
import { errorMessage } from './error-message.mjs';
import { fileURLToPath } from 'url';
import { flagArgs, validateFlags } from './cli-flags.mjs';
import { cdpHttp, octocodeWorkspace } from './engine-io.mjs';

const __dir = dirname(fileURLToPath(import.meta.url));
const argv = process.argv.slice(2);
const { getArg, hasFlag } = flagArgs(argv);

if (hasFlag('--help') || hasFlag('-h') || argv.length === 0) {
  console.error(`Usage: node cookie-bridge.mjs --i-understand-secrets --to-port 9222 \\
  (--from-port 9333 | --from-profile Default | --from-storage-state state.json) \\
  [--urls "https://example.com"] [--export-storage-state out.json] [--dry-run]
  [--source-port 9333] [--chromePath <browser for --from-profile>]

Gate: --i-understand-secrets is required. Values are never printed.
Prefer --from-port (existing CDP) or --from-storage-state. --from-profile needs the browser fully quit
and fails on Google Chrome 136+, which refuses remote debugging on its real profile.`);
  process.exit(argv.length === 0 ? 1 : 0);
}

validateFlags(
  argv,
  [
    '--to-port',
    '--from-port',
    '--from-profile',
    '--from-storage-state',
    '--export-storage-state',
    '--source-port',
    '--chromePath',
    '--urls',
  ],
  ['--help', '-h', '--i-understand-secrets', '--dry-run']
);
const ACK = hasFlag('--i-understand-secrets');
const TO_PORT = getArg('--to-port', '');
const FROM_PORT = getArg('--from-port', '');
const FROM_PROFILE = getArg('--from-profile', '');
const FROM_STATE = getArg('--from-storage-state', '');
const EXPORT_STATE = getArg('--export-storage-state', '');
const SOURCE_PORT = getArg('--source-port', '9333');
const CHROME_PATH = getArg('--chromePath', '');
const URLS = getArg('--urls', '')
  .split(',')
  .map(s => s.trim())
  .filter(Boolean);
const DRY_RUN = hasFlag('--dry-run');
const WORKSPACE_OUTPUT_BASE = octocodeWorkspace();

function fail(msg: string) {
  console.error(`[COOKIE_BRIDGE] ${msg}`);
  process.exit(1);
}

if (!ACK)
  fail(
    'Refusing: pass --i-understand-secrets after user approval (CDP can read session secrets).'
  );
if (!TO_PORT && !EXPORT_STATE)
  fail('Need --to-port and/or --export-storage-state.');
const sources = [FROM_PORT, FROM_PROFILE, FROM_STATE].filter(Boolean);
if (sources.length !== 1)
  fail(
    'Pick exactly one of --from-port, --from-profile, --from-storage-state.'
  );
if (FROM_PROFILE && TO_PORT === SOURCE_PORT)
  fail('--source-port must differ from --to-port.');
if (EXPORT_STATE) {
  const exportPath = resolve(EXPORT_STATE);
  const exportRelative = relative(WORKSPACE_OUTPUT_BASE, exportPath);
  if (exportRelative.startsWith('..') || isAbsolute(exportRelative)) {
    fail(`--export-storage-state must stay under ${WORKSPACE_OUTPUT_BASE}`);
  }
}

const WS = globalThis.WebSocket;
const [nodeMajor] = process.versions.node.split('.').map(Number);
if (!Number.isFinite(nodeMajor) || nodeMajor < 24 || !WS) {
  fail(
    `Node.js 24+ required (you have ${process.versions.node}). Native WebSocket is unavailable.`
  );
}

/** Browser-level session: Storage cookies cover every context and partition. */
async function connectBrowser(port: string) {
  const version = await cdpHttp(port, '/json/version');
  if (!version.webSocketDebuggerUrl)
    throw new Error(`No browser WS on port ${port}`);
  return connectCDP(version.webSocketDebuggerUrl);
}

async function readCookiesFromPort(port: string): Promise<CookieRecord[]> {
  const browser = await connectBrowser(port);
  try {
    return (await browser.send('Storage.getCookies', {})).cookies || [];
  } finally {
    browser.close();
  }
}

/**
 * Sets every cookie, falling back to one call per cookie when the batch is rejected,
 * then reads the jar back so a cookie Chrome dropped is reported instead of assumed.
 */
async function writeCookiesToPort(port: string, cookies: CookieRecord[]) {
  const browser = await connectBrowser(port);
  try {
    const payload = cookies.map(c => toSetCookie(c, URLS));
    const rejected: { name: string; key: string; reason: string }[] = [];
    try {
      await browser.send('Storage.setCookies', { cookies: payload });
    } catch {
      for (const cookie of payload) {
        try {
          await browser.send('Storage.setCookies', { cookies: [cookie] });
        } catch (error) {
          rejected.push({
            name: cookie.name,
            key: cookieKey(cookie),
            reason: errorMessage(error),
          });
        }
      }
    }
    const stored = new Set(
      ((await browser.send('Storage.getCookies', {})).cookies || []).map(
        cookieKey
      )
    );
    const missing = payload
      .filter(c => !rejected.some(r => r.key === cookieKey(c)))
      .filter(c => !stored.has(cookieKey(c)))
      .map(c => c.name);
    return { attempted: payload.length, rejected, missing };
  } finally {
    browser.close();
  }
}

function writeStorageState(path: string, cookies: CookieRecord[]) {
  mkdirSync(dirname(resolve(path)), { recursive: true, mode: 0o700 });
  writeFileSync(
    path,
    `${JSON.stringify({ cookies: cookies.map(toStorageStateCookie) }, null, 2)}\n`,
    { mode: 0o600 }
  );
}

/** Launches the real profile; refuses a reused or isolated browser on the source port. */
function launchProfileSource(profile: string, port: string) {
  return new Promise<void>((resolveLaunch, rejectLaunch) => {
    const openBrowser = join(__dir, 'open-browser.mjs');
    const args = [
      openBrowser,
      '--profile',
      profile,
      '--port',
      port,
      '--url',
      'about:blank',
      ...(CHROME_PATH ? ['--chromePath', CHROME_PATH] : []),
    ];
    console.error(
      `[COOKIE_BRIDGE] Launching profile=${profile} on port=${port} (the browser must not hold this profile)`
    );
    const child = spawn(process.execPath, args, {
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    let stdout = '';
    let stderr = '';
    child.stdout.on('data', d => {
      stdout += d;
    });
    child.stderr.on('data', d => {
      stderr += d;
    });
    child.on('exit', code => {
      const status = stdout
        .split('\n')
        .filter(line => line.startsWith('{'))
        .map(line => {
          try {
            return JSON.parse(line);
          } catch {
            return null;
          }
        })
        .filter(Boolean)
        .pop();
      if (code !== 0 || status?.status !== 'BROWSER_READY') {
        rejectLaunch(
          new Error(
            `open-browser failed (exit ${code}): ${status?.message ?? (stdout + stderr).slice(-500)}`
          )
        );
      } else if (status.reused) {
        rejectLaunch(
          new Error(
            `--source-port ${port} already has a browser; it is not the requested profile. Pick a free --source-port or use --from-port ${port}.`
          )
        );
      } else if (status.isolated) {
        rejectLaunch(
          new Error(
            'open-browser started an isolated profile, not the real one'
          )
        );
      } else resolveLaunch();
    });
  });
}

/** Closes the real-profile browser this run launched so its CDP port stops serving. */
async function closeSourceBrowser(port: string) {
  try {
    const browser = await connectBrowser(port);
    await browser.send('Browser.close').catch(() => {});
    browser.close();
  } catch {}
  for (let i = 0; i < 50; i++) {
    try {
      await cdpHttp(port, '/json/version');
    } catch {
      return true;
    }
    await new Promise(r => setTimeout(r, 200));
  }
  return false;
}

async function main() {
  console.log('[ACTION] cookie-bridge start (values redacted)');

  let launchedSource = false;
  let cookies: CookieRecord[] = [];

  try {
    if (FROM_STATE) {
      if (!existsSync(FROM_STATE))
        throw new Error(`storage state not found: ${FROM_STATE}`);
      cookies = readStorageStateCookies(
        JSON.parse(readFileSync(FROM_STATE, 'utf8'))
      );
      console.log(`[METRIC] loaded storage-state path=${FROM_STATE}`);
    } else if (FROM_PORT) {
      cookies = await readCookiesFromPort(FROM_PORT);
      console.log(`[METRIC] loaded from-port=${FROM_PORT}`);
    } else if (FROM_PROFILE) {
      await launchProfileSource(FROM_PROFILE, SOURCE_PORT);
      launchedSource = true;
      cookies = await readCookiesFromPort(SOURCE_PORT);
      console.log(
        `[METRIC] loaded from-profile=${FROM_PROFILE} source-port=${SOURCE_PORT}`
      );
    }

    cookies = filterCookies(cookies, URLS);
    const metas = cookies.map(cookieMeta);
    console.log(
      `[METRIC] cookies=${metas.length} domains=${new Set(metas.map(m => m.domain)).size} partitioned=${metas.filter(m => m.partitioned).length}`
    );
    console.log(`[FINDING] cookie-names=${metas.map(m => m.name).join(',')}`);

    if (EXPORT_STATE) {
      writeStorageState(EXPORT_STATE, cookies);
      console.log(
        `[ARTIFACT] storage-state ${resolve(EXPORT_STATE)} mode=0600`
      );
    }

    if (DRY_RUN) {
      console.log('[REASON] dry-run: no inject');
      return;
    }

    if (TO_PORT) {
      const { attempted, rejected, missing } = await writeCookiesToPort(
        TO_PORT,
        cookies
      );
      const stored = attempted - rejected.length - missing.length;
      for (const r of rejected)
        console.log(`[FINDING] COOKIE_REJECTED name=${r.name} ${r.reason}`);
      if (missing.length)
        console.log(
          `[FINDING] COOKIE_NOT_STORED names=${missing.join(',')} (expired or refused by the target browser)`
        );
      console.log(
        `[AUTH_COMPLETE] injected=${stored}/${attempted} to-port=${TO_PORT}`
      );
      console.log(
        '[REASON] navigate/target app on to-port; do not print cookie values'
      );
      if (stored < attempted) process.exitCode = 1;
    }
  } finally {
    if (launchedSource) {
      const closed = await closeSourceBrowser(SOURCE_PORT);
      console.log(
        closed
          ? `[ACTION] closed source browser on port=${SOURCE_PORT}`
          : `[FINDING] SOURCE_STILL_RUNNING port=${SOURCE_PORT}: quit that browser; its CDP port exposes the real profile`
      );
    }
  }
}

main().catch(err => {
  console.error(`[EXCEPTION] ${err.message}`);
  process.exit(1);
});
