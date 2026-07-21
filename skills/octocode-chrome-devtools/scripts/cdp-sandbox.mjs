#!/usr/bin/env node

import { spawn, spawnSync }               from 'child_process';
import { createRequire }                  from 'module';
import { resolve, dirname, join }         from 'path';
import { fileURLToPath }                  from 'url';
import { existsSync, realpathSync,
         mkdirSync, copyFileSync }        from 'fs';
import { getOctocodeHome, propagateOctocodeEnv } from '@octocodeai/config';

/**
 * `--allow-net` exists only on Node 25+ (Permission Model network scope).
 * Prefer version gate; confirm via --help when version is ambiguous/custom builds.
 */
function nodeSupportsAllowNet() {
  const [major] = process.versions.node.split('.').map(Number);
  if (Number.isFinite(major) && major >= 25) return true;
  if (Number.isFinite(major) && major < 25) return false;
  const help = spawnSync(process.execPath, ['--help'], {
    encoding: 'utf8',
    timeout: 5000,
  });
  const text = `${help.stdout || ''}${help.stderr || ''}`;
  return /--allow-net\b/.test(text);
}

function requireNode22() {
  const [major] = process.versions.node.split('.').map(Number);
  if (!Number.isFinite(major) || major < 22) {
    console.error(`[CDP_SANDBOX] Node.js 22+ required (you have ${process.versions.node}).`);
    process.exit(1);
  }
}
requireNode22();

const __dir  = dirname(fileURLToPath(import.meta.url));
const RUNNER = resolve(__dir, 'cdp-runner.mjs');
const requireForResolve = createRequire(import.meta.url);
const CONFIG_ENTRY = requireForResolve.resolve('@octocodeai/config');
// Node's permission model must allow the resolved package path and the
// workspace symlink path; Yarn/workspace installs can use either at runtime.
const CONFIG_ROOT = resolve(dirname(CONFIG_ENTRY), '..');
const CONFIG_NODE_MODULES_ROOT = resolve(process.cwd(), 'node_modules/@octocodeai/config');

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

propagateOctocodeEnv({ cwd: process.cwd(), trusted: true });

function octocodeOutputBase() {
  const workspace = resolve(process.cwd(), '.octocode');
  try {
    mkdirSync(workspace, { recursive: true, mode: 0o700 });
    return workspace;
  } catch {
    const home = getOctocodeHome();
    mkdirSync(home, { recursive: true, mode: 0o700 });
    return home;
  }
}

const OCTOCODE_OUTPUT_BASE = octocodeOutputBase();
const timestamp = new Date().toISOString().slice(0, 19).replace(/[:T]/g, '-');
const OUTPUT_DIR = join(OCTOCODE_OUTPUT_BASE, 'chrome-devtools', timestamp);
mkdirSync(OUTPUT_DIR, { recursive: true, mode: 0o700 });
const SESSION_META_DIR = join(OCTOCODE_OUTPUT_BASE, 'chrome-devtools', 'session-meta', `port-${PORT}`);
mkdirSync(SESSION_META_DIR, { recursive: true, mode: 0o700 });

const safePath = (p) => { try { return realpathSync(p); } catch { return p; } };

const TMPDIR_RAW  = OCTOCODE_OUTPUT_BASE;
const TMPDIR_REAL = safePath(TMPDIR_RAW);
const RUNNER_REAL = safePath(RUNNER);
const CONFIG_ROOT_REAL = safePath(CONFIG_ROOT);
const CONFIG_NODE_MODULES_ROOT_REAL = safePath(CONFIG_NODE_MODULES_ROOT);
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

const readPaths  = [...new Set([
  RUNNER,
  RUNNER_REAL,
  CONFIG_ROOT,
  CONFIG_ROOT_REAL,
  CONFIG_NODE_MODULES_ROOT,
  CONFIG_NODE_MODULES_ROOT_REAL,
  TMPDIR_RAW,
  TMPDIR_REAL,
  ...allowReadExtra,
])];
const writePaths = [...new Set([
  TMPDIR_RAW,
  TMPDIR_REAL,
  OUTPUT_DIR,
  OUTPUT_REAL,
  SESSION_META_DIR,
  SESSION_META_REAL,
])];

const allowNet = nodeSupportsAllowNet();
const permFlags = [
  '--permission',
  ...(allowNet ? ['--allow-net'] : []),
  ...readPaths.map(p  => `--allow-fs-read=${p}`),
  ...writePaths.map(p => `--allow-fs-write=${p}`),
];

// Keep the sandbox hermetic: pass only documented knobs used by examples,
// never the parent env where tokens/cookies may live.
const SCRIPT_ENV_ALLOWLIST = [
  'MONITOR_MS',
  'SLOW_MS',
  'MAX_STDOUT_ITEMS',
  'DOM_SELECTOR',
  'DOM_ACTION',
  'DOM_VALUE',
  'DOM_STABILITY_MS',
];
const scriptEnv = Object.fromEntries(
  SCRIPT_ENV_ALLOWLIST
    .filter(key => process.env[key] !== undefined)
    .map(key => [key, process.env[key]])
);

const childEnv = {
  CDP_OUTPUT_DIR: OUTPUT_DIR,
  CDP_SESSION_META_DIR: SESSION_META_DIR,
  TMPDIR: OCTOCODE_OUTPUT_BASE,
  TMP: OCTOCODE_OUTPUT_BASE,
  TEMP: OCTOCODE_OUTPUT_BASE,
  ...scriptEnv,
  ...(process.env.SystemRoot ? { SystemRoot: process.env.SystemRoot } : {}),
  ...(process.env.WINDIR ? { WINDIR: process.env.WINDIR } : {}),
};

console.error('[CDP_SANDBOX] Launching runner in sandbox (Node.js Permission Model)');
console.error(`[CDP_SANDBOX]  Output dir:    ${OUTPUT_DIR}`);
console.error(`[CDP_SANDBOX]  Session meta:  ${SESSION_META_DIR}`);
console.error(`[CDP_SANDBOX]  FS write:      output dir + session meta dir (mode 0700)`);
console.error(`[CDP_SANDBOX]  FS read:       .octocode output tree + runner`);
console.error(`[CDP_SANDBOX]  child_process: blocked`);
console.error(`[CDP_SANDBOX]  workers:       blocked`);
console.error(`[CDP_SANDBOX]  env:           minimal allowlist (parent env not inherited)`);
console.error(`[CDP_SANDBOX]  Node:           ${process.versions.node}`);
console.error(`[CDP_SANDBOX]  Network:       CDP localhost only; --allow-net=${allowNet ? 'yes (Node 25+)' : 'skipped (Node <25)'}`);
if (!allowNet) {
  console.error('[CDP_SANDBOX]  Note: Node 22–24 grant net under --permission; Node 25+ requires --allow-net');
}

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
