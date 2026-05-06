#!/usr/bin/env node
// Run cdp-runner.mjs with Node permissions, a per-run output dir, and an overall script timeout.
//
// --script-timeout <ms>  Overall script timeout (default: 300000 / 5 min). Kills the entire runner
//                        if it hangs. cdp-runner's --timeout is per CDP method call only.

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
// Overall script timeout (ms). --timeout in cdp-runner.mjs is per-CDP-method; this kills the
// entire sandboxed process if it runs longer than SCRIPT_TIMEOUT_MS.
// Default: 300 000 ms (5 min). Override with --script-timeout <ms>.
const SCRIPT_TIMEOUT_MS = parseInt(getArg('--script-timeout', '300000'), 10);

if (!scriptArg && !LIST_TARGETS) {
  console.error('[CDP_SANDBOX] Usage: node cdp-sandbox.mjs <script.mjs> [--port 9222] [options]');
  console.error('[CDP_SANDBOX] Options are the same as cdp-runner.mjs');
  process.exit(1);
}

const timestamp = new Date().toISOString().slice(0, 19).replace(/[:T]/g, '-');
const OUTPUT_DIR = join(tmpdir(), '.octocode-chrome-devtools', timestamp);
mkdirSync(OUTPUT_DIR, { recursive: true, mode: 0o700 });

// Node 22 normalises --allow-fs-* entries to realpaths internally, so we must
// pass the realpath AND also spawn the runner/script via their realpath forms.
// On macOS: $TMPDIR=/var/folders/… but realpath=/private/var/folders/…
const safePath = (p) => { try { return realpathSync(p); } catch { return p; } };

const TMPDIR_RAW  = tmpdir();
const TMPDIR_REAL = safePath(TMPDIR_RAW);
const RUNNER_REAL = safePath(RUNNER);
const OUTPUT_REAL = safePath(OUTPUT_DIR);

// Generated scripts import helpers via `new URL('./<helper>.mjs', import.meta.url)`,
// which resolves relative to the script's location in $TMPDIR. Keep filenames fixed so that
// paths always resolve correctly. Concurrent runs are safe: these are read-only utility files.
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

// Replace the script arg in argv with its realpath so the runner accesses it
// via a path that matches the allow list.
const spawnArgv = argv.map(a => (a === scriptArg && scriptReal) ? scriptReal : a);

// --allow-fs-read:  runner + entire $TMPDIR tree (covers scripts + output dir)
// --allow-fs-write: output dir only — NOT all of $TMPDIR (tighter than before)
// --allow-child-process / --allow-worker: NOT granted
// --allow-net:      Node's PM has no network scoping flag in any stable release.
//                   localhost-only restriction is enforced in cdp-runner.mjs by
//                   patching globalThis.fetch and globalThis.WebSocket before the
//                   user script runs.

// Deduplicate — include both symlink and realpath forms for every path
const readPaths  = [...new Set([RUNNER, RUNNER_REAL, TMPDIR_RAW, TMPDIR_REAL, ...allowReadExtra])];
const writePaths = [...new Set([OUTPUT_DIR, OUTPUT_REAL])];

const permFlags = [
  '--permission',
  ...readPaths.map(p  => `--allow-fs-read=${p}`),
  ...writePaths.map(p => `--allow-fs-write=${p}`),
];

const childEnv = {
  CDP_OUTPUT_DIR: OUTPUT_DIR,
  ...(process.env.TMPDIR ? { TMPDIR: process.env.TMPDIR } : {}),
  ...(process.env.TMP ? { TMP: process.env.TMP } : {}),
  ...(process.env.TEMP ? { TEMP: process.env.TEMP } : {}),
  ...(process.env.SystemRoot ? { SystemRoot: process.env.SystemRoot } : {}),
  ...(process.env.WINDIR ? { WINDIR: process.env.WINDIR } : {}),
};

console.error('[CDP_SANDBOX] Launching runner in sandbox (Node.js Permission Model)');
console.error(`[CDP_SANDBOX]  Output dir:    ${OUTPUT_DIR}`);
console.error(`[CDP_SANDBOX]  FS write:      output dir only (mode 0700)`);
console.error(`[CDP_SANDBOX]  FS read:       $TMPDIR tree + runner`);
console.error(`[CDP_SANDBOX]  child_process: blocked`);
console.error(`[CDP_SANDBOX]  workers:       blocked`);
console.error(`[CDP_SANDBOX]  env:           minimal allowlist (parent env not inherited)`);
console.error(`[CDP_SANDBOX]  Network:       localhost only (fetch+WebSocket patched in runner)`);

const child = spawn(process.execPath, [...permFlags, RUNNER_REAL, ...spawnArgv], {
  stdio: 'inherit',
  env:   childEnv,
});

// Overall script timeout — kills the runner process if it exceeds SCRIPT_TIMEOUT_MS.
// This is separate from cdp-runner's per-method --timeout flag.
const scriptTimer = setTimeout(() => {
  console.error(`[CDP_SANDBOX] Script timeout after ${SCRIPT_TIMEOUT_MS}ms — killing runner`);
  child.kill('SIGTERM');
  setTimeout(() => child.kill('SIGKILL'), 2000).unref();
}, SCRIPT_TIMEOUT_MS);
scriptTimer.unref(); // don't keep the process alive on its own

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
