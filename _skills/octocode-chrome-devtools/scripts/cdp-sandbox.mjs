#!/usr/bin/env node

import { spawn }                          from 'child_process';
import { resolve, dirname, join }         from 'path';
import { fileURLToPath }                  from 'url';
import { existsSync, realpathSync,
         mkdirSync, copyFileSync }        from 'fs';
import { tmpdir }                         from 'os';

const __dir  = dirname(fileURLToPath(import.meta.url));
const RUNNER = resolve(__dir, 'cdp-runner.mjs');

const argv     = process.argv.slice(2);
const getArg   = (flag, def) => { const i = argv.indexOf(flag); return i !== -1 && argv[i + 1] ? argv[i + 1] : def; };
const hasFlag  = (flag) => argv.includes(flag);

const PORT         = getArg('--port', '9222');
const LIST_TARGETS = hasFlag('--list-targets');
const scriptArg    = argv.find(a => !a.startsWith('--') && (a.endsWith('.mjs') || a.endsWith('.js')));
const SCRIPT_TIMEOUT_MS = parseInt(getArg('--script-timeout', '300000'), 10);

if (!scriptArg && !LIST_TARGETS) {
  console.error('[CDP_SANDBOX] Usage: node cdp-sandbox.mjs <script.mjs> [--port 9222] [options]');
  console.error('[CDP_SANDBOX] Options are the same as cdp-runner.mjs');
  process.exit(1);
}

const timestamp = new Date().toISOString().slice(0, 19).replace(/[:T]/g, '-');
const OUTPUT_DIR = join(tmpdir(), '.octocode-chrome-devtools', timestamp);
mkdirSync(OUTPUT_DIR, { recursive: true, mode: 0o700 });
const SESSION_META_DIR = join(tmpdir(), '.octocode-chrome-devtools', 'session-meta', `port-${PORT}`);
mkdirSync(SESSION_META_DIR, { recursive: true, mode: 0o700 });

const safePath = (p) => { try { return realpathSync(p); } catch { return p; } };

const TMPDIR_RAW  = tmpdir();
const TMPDIR_REAL = safePath(TMPDIR_RAW);
const RUNNER_REAL = safePath(RUNNER);
const OUTPUT_REAL = safePath(OUTPUT_DIR);
const SESSION_META_REAL = safePath(SESSION_META_DIR);

const HELPERS = ['sourcemap-resolver.mjs', 'undercover.mjs'];
for (const helper of HELPERS) {
  const src = resolve(__dir, helper);
  const dst = join(TMPDIR_RAW, helper);
  if (existsSync(src)) {
    try { copyFileSync(src, dst); }
    catch (e) { console.error(`[CDP_SANDBOX] Warning: could not copy ${helper}: ${e.message}`); }
  }
}

let scriptReal = null;
const allowReadExtra = [];
if (scriptArg) {
  const scriptPath = resolve(process.cwd(), scriptArg);
  if (!existsSync(scriptPath)) {
    console.error(`[CDP_SANDBOX] Script not found: ${scriptPath}`);
    process.exit(1);
  }
  scriptReal = safePath(scriptPath);
  allowReadExtra.push(scriptPath, scriptReal);
}

const spawnArgv = argv.map(a => (a === scriptArg && scriptReal) ? scriptReal : a);

const readPaths  = [...new Set([RUNNER, RUNNER_REAL, TMPDIR_RAW, TMPDIR_REAL, ...allowReadExtra])];
const writePaths = [...new Set([OUTPUT_DIR, OUTPUT_REAL, SESSION_META_DIR, SESSION_META_REAL])];

const permFlags = [
  '--permission',
  ...readPaths.map(p  => `--allow-fs-read=${p}`),
  ...writePaths.map(p => `--allow-fs-write=${p}`),
];

const childEnv = {
  CDP_OUTPUT_DIR: OUTPUT_DIR,
  CDP_SESSION_META_DIR: SESSION_META_DIR,
  ...(process.env.TMPDIR ? { TMPDIR: process.env.TMPDIR } : {}),
  ...(process.env.TMP ? { TMP: process.env.TMP } : {}),
  ...(process.env.TEMP ? { TEMP: process.env.TEMP } : {}),
  ...(process.env.SystemRoot ? { SystemRoot: process.env.SystemRoot } : {}),
  ...(process.env.WINDIR ? { WINDIR: process.env.WINDIR } : {}),
};

console.error('[CDP_SANDBOX] Launching runner in sandbox (Node.js Permission Model)');
console.error(`[CDP_SANDBOX]  Output dir:    ${OUTPUT_DIR}`);
console.error(`[CDP_SANDBOX]  Session meta:  ${SESSION_META_DIR}`);
console.error(`[CDP_SANDBOX]  FS write:      output dir + session meta dir (mode 0700)`);
console.error(`[CDP_SANDBOX]  FS read:       $TMPDIR tree + runner`);
console.error(`[CDP_SANDBOX]  child_process: blocked`);
console.error(`[CDP_SANDBOX]  workers:       blocked`);
console.error(`[CDP_SANDBOX]  env:           minimal allowlist (parent env not inherited)`);
console.error(`[CDP_SANDBOX]  Network:       localhost only (fetch+WebSocket patched in runner)`);

const child = spawn(process.execPath, [...permFlags, RUNNER_REAL, ...spawnArgv], {
  stdio: 'inherit',
  env:   childEnv,
});

const scriptTimer = setTimeout(() => {
  console.error(`[CDP_SANDBOX] Script timeout after ${SCRIPT_TIMEOUT_MS}ms - killing runner`);
  child.kill('SIGTERM');
  setTimeout(() => child.kill('SIGKILL'), 2000).unref();
}, SCRIPT_TIMEOUT_MS);
scriptTimer.unref();

child.on('exit', (code, signal) => {
  clearTimeout(scriptTimer);
  if (signal) {
    console.error(`[CDP_SANDBOX] Runner killed by signal: ${signal}`);
    process.exit(1);
  }
  process.exit(code ?? 0);
});

child.on('error', (err) => {
  console.error(`[CDP_SANDBOX] Failed to launch sandboxed runner: ${err.message}`);
  process.exit(1);
});
