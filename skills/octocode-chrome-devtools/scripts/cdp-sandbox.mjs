#!/usr/bin/env node

import { spawn, spawnSync }               from 'child_process';
import { resolve, dirname, join }         from 'path';
import { fileURLToPath }                  from 'url';
import { existsSync, realpathSync,
         mkdirSync, copyFileSync }        from 'fs';
import { getOctocodeHome, propagateOctocodeEnv } from './octocode-config.mjs';

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
// Config is vendored inside the skill folder so the skill runs standalone with
// no npm install; Node's permission model must allow both it and its realpath.
const CONFIG_ENTRY = resolve(__dir, 'octocode-config.mjs');

const argv     = process.argv.slice(2);
const getArg   = (flag, def) => { const i = argv.indexOf(flag); return i !== -1 && argv[i + 1] ? argv[i + 1] : def; };
const hasFlag  = (flag) => argv.includes(flag);

const PORT         = getArg('--port', '9222');
const LIST_TARGETS = hasFlag('--list-targets');
const scriptArg    = argv.find(a => !a.startsWith('--') && (a.endsWith('.mjs') || a.endsWith('.js')));
const SCRIPT_TIMEOUT_MS = parseInt(getArg('--script-timeout', '300000'), 10);
const VERBOSE      = hasFlag('--verbose');

if (hasFlag('--help') || hasFlag('-h')) {
  console.error('[CDP_SANDBOX] Usage: node cdp-sandbox.mjs <script.mjs> [--port 9222] [options]');
  console.error('[CDP_SANDBOX] Options are the same as cdp-runner.mjs');
  process.exit(0);
}

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
const OUTPUT_DIR = join(OCTOCODE_OUTPUT_BASE, 'tmp', 'chrome-devtools', timestamp);
mkdirSync(OUTPUT_DIR, { recursive: true, mode: 0o700 });
const SESSION_META_DIR = join(OCTOCODE_OUTPUT_BASE, 'tmp', 'chrome-devtools', 'session-meta', `port-${PORT}`);
mkdirSync(SESSION_META_DIR, { recursive: true, mode: 0o700 });

const safePath = (p) => { try { return realpathSync(p); } catch { return p; } };

const TMPDIR_RAW  = OCTOCODE_OUTPUT_BASE;
const TMPDIR_REAL = safePath(TMPDIR_RAW);
const RUNNER_REAL = safePath(RUNNER);
const CONFIG_ENTRY_REAL = safePath(CONFIG_ENTRY);
const OUTPUT_REAL = safePath(OUTPUT_DIR);
const SESSION_META_REAL = safePath(SESSION_META_DIR);

const HELPERS = ['sourcemap-resolver.mjs', 'undercover.mjs', 'mandatory-stealth.mjs', 'human-input.mjs', 'dom-actionability.mjs'];
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
  resolve(__dir, 'mandatory-stealth.mjs'),
  resolve(__dir, 'undercover.mjs'),
  CONFIG_ENTRY,
  CONFIG_ENTRY_REAL,
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
  'DOM_REF',
  'DOM_ACTION',
  'DOM_VALUE',
  'DOM_STABILITY_MS',
  'SNAPSHOT_DEPTH',
  'SNAPSHOT_MAX',
  'WEBMCP_ACTION',
  'WEBMCP_TOOL',
  'WEBMCP_INPUT',
  'WEBMCP_FRAME',
  'WEBMCP_WAIT_MS',
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
  ...(VERBOSE ? { CDP_VERBOSE: '1' } : {}),
  ...(process.env.SystemRoot ? { SystemRoot: process.env.SystemRoot } : {}),
  ...(process.env.WINDIR ? { WINDIR: process.env.WINDIR } : {}),
};

if (VERBOSE) {
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
} else {
  console.error(`[CDP_SANDBOX] sandboxed (node ${process.versions.node}, fs scoped, net=CDP-only) — rerun with --verbose for full detail`);
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
