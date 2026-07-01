// octocode-agent launcher core.
//
// Architecture (RFC .octocode/rfc/octocode-pi-harness, Alternative C / G8):
//   - @octocodeai/pi-extension is THE CORE (system prompt, skills, tools, awareness).
//   - octocode-agent is the PLATFORM: it bundles Pi + the core as dependencies and
//     launches Pi with the core loaded in replace mode. Updating the core (this
//     package's @octocodeai/pi-extension dependency) updates the agent — automatically
//     on a platform release, or on demand via `octocode-agent update`.
//
// This module has NO side effects at import time (safe to unit-test); the executable
// wrapper (octocode-agent.mjs) calls main(process.argv.slice(2)).

import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';

const require = createRequire(import.meta.url);

export const CORE_PACKAGE = '@octocodeai/pi-extension';
export const PI_PACKAGE = '@earendil-works/pi-coding-agent';

/** Read this launcher package's own version. */
export function launcherVersion() {
  try {
    const pkgPath = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', 'package.json');
    return JSON.parse(fs.readFileSync(pkgPath, 'utf8')).version ?? null;
  } catch {
    return null;
  }
}

/**
 * Resolve an installed dependency's package.json path (or null).
 * Note: packages with an `exports` field block `require.resolve('<pkg>/package.json')`
 * unless they export it. So we fall back to resolving the package entry and walking up
 * to the owning package.json (matched by `name`).
 */
export function resolvePackageJson(pkgName) {
  try {
    return require.resolve(`${pkgName}/package.json`);
  } catch {
    /* exports-gated — fall through */
  }
  // Scan the node_modules search chain for <pkg>/package.json. This is independent of
  // the package's `exports` map and its condition set — Pi's "." export defines only
  // `import` (ESM), which a CJS require.resolve of the entry cannot satisfy.
  const searchPaths = require.resolve.paths(pkgName) || [];
  const segments = pkgName.split('/');
  for (const base of searchPaths) {
    const pj = path.join(base, ...segments, 'package.json');
    if (fs.existsSync(pj)) return pj;
  }
  return null;
}

/** Read an installed dependency's version (or null if not resolvable). */
export function readPackageVersion(pkgName) {
  const pkgJson = resolvePackageJson(pkgName);
  if (!pkgJson) return null;
  try {
    return JSON.parse(fs.readFileSync(pkgJson, 'utf8')).version ?? null;
  } catch {
    return null;
  }
}

/** Absolute path to the core extension's package root, or null. */
export function resolveCoreRoot() {
  const pkgJson = resolvePackageJson(CORE_PACKAGE);
  return pkgJson ? path.dirname(pkgJson) : null;
}

/**
 * Resolve the Pi host executable from its installed package.json `bin` field.
 * Returns { bin, pkgRoot } or null. Kept data-driven so a Pi bump can't silently
 * break us on a hardcoded path.
 */
export function resolvePiBin() {
  const pkgJson = resolvePackageJson(PI_PACKAGE);
  if (!pkgJson) return null;
  const pkgRoot = path.dirname(pkgJson);
  let manifest;
  try {
    manifest = JSON.parse(fs.readFileSync(pkgJson, 'utf8'));
  } catch {
    return null;
  }
  const binField = manifest.bin;
  let binRel = null;
  if (typeof binField === 'string') binRel = binField;
  else if (binField && typeof binField === 'object') binRel = binField.pi ?? Object.values(binField)[0] ?? null;
  if (!binRel) return null;
  const bin = path.resolve(pkgRoot, binRel);
  return fs.existsSync(bin) ? { bin, pkgRoot } : null;
}

/**
 * Build the environment Pi launches with.
 * - OCTOCODE_PROMPT_MODE=replace: the core leads the system prompt (branded agent).
 * - OCTOCODE_AGENT=1: a marker the core / hooks can key off.
 * Never clobbers a value the user explicitly set.
 */
export function buildLaunchEnv(baseEnv = process.env) {
  const env = { ...baseEnv };
  if (!env.OCTOCODE_PROMPT_MODE) env.OCTOCODE_PROMPT_MODE = 'replace';
  env.OCTOCODE_AGENT = '1';
  return env;
}

/** Parse the argv into a launcher invocation. Reserved subcommands win; everything else is forwarded to Pi. */
export function parseInvocation(argv = []) {
  const first = argv[0];
  if (first === 'update' || first === '--update') {
    const target = argv[1] === 'core' ? 'core' : 'platform';
    return { command: 'update', target };
  }
  if (first === '--version' || first === '-v' || first === 'version') {
    return { command: 'version' };
  }
  if (first === '--agent-help') {
    return { command: 'help' };
  }
  return { command: 'run', rest: argv };
}

export function versionReport() {
  const lines = [
    `octocode-agent   ${launcherVersion() ?? '?'}`,
    `core (${CORE_PACKAGE})   ${readPackageVersion(CORE_PACKAGE) ?? 'not installed'}`,
    `pi host (${PI_PACKAGE})   ${readPackageVersion(PI_PACKAGE) ?? 'not installed'}`,
  ];
  return lines.join('\n');
}

export function helpReport() {
  return [
    'octocode-agent — self-working coding agent (Pi + Octocode harness core)',
    '',
    'Usage:',
    '  octocode-agent [pi args...]   Launch the agent (forwards args to the Pi host)',
    '  octocode-agent update         Self-update the platform (pulls the newest core)',
    '  octocode-agent update core    Update only the core (@octocodeai/pi-extension) in place',
    '  octocode-agent --version      Print launcher, core, and Pi host versions',
    '',
    'The core (@octocodeai/pi-extension) carries the prompt, skills, tools, and memory.',
    'Updating it updates the agent.',
  ].join('\n');
}

// ── side-effecting runners (thin wrappers around spawnSync so tests can inject) ──

function run(cmd, args, options, spawn = spawnSync) {
  return spawn(cmd, args, { stdio: 'inherit', ...options });
}

// Lean-tool set (RFC G2): the agent keeps the 4 core built-ins (read, bash, edit, write)
// plus every extension/custom tool the core registers — notably `web` (search + fetch)
// and the memory tools. Discovery built-ins grep/find/ls are dropped so the agent routes
// discovery through Octocode + `web`, with bash still reaching rg/find when needed.
// We enforce this with `--exclude-tools` (not `--tools`): exclude drops ONLY the named
// tools and leaves custom/extension tools active, so the launcher never has to enumerate
// the core's tool names — `web` stays wired in without coupling.
export const LEAN_EXCLUDE_TOOLS = ['grep', 'find', 'ls'];

/**
 * Build the Pi argv: load the core (extension + its packaged skills) for this run
 * via `-e <spec>` — an ephemeral, per-run load with NO global settings mutation and
 * no trust prompt for our own package. `-e` on a directory loads resources by package
 * rules, so a single flag brings the extension AND the bundled skills.
 * --no-extensions is ALWAYS passed so that globally-installed extensions (including a
 * globally-installed @octocodeai/pi-extension) cannot conflict with the bundled core.
 * OCTOCODE_AGENT_CLEAN=1 additionally passes --no-skills so ONLY the Octocode harness
 * loads (fully deterministic branded agent, no user skills).
 * OCTOCODE_AGENT_FULL_TOOLS=1 keeps grep/find/ls (opt out of the lean set).
 */
export function buildPiArgs(spec, argv = [], env = {}) {
  const args = ['--no-extensions'];
  if (env.OCTOCODE_AGENT_CLEAN === '1') args.push('--no-skills');
  if (env.OCTOCODE_AGENT_FULL_TOOLS !== '1') args.push('--exclude-tools', LEAN_EXCLUDE_TOOLS.join(','));
  args.push('-e', spec);
  return [...args, ...argv];
}

/**
 * Launch the Pi host with the core loaded in replace mode. Returns the child exit code.
 */
export function launchAgent(argv = [], deps = {}) {
  const spawn = deps.spawn ?? spawnSync;
  const log = deps.log ?? console.error;
  const piInfo = (deps.resolvePiBin ?? resolvePiBin)();
  if (!piInfo) {
    log(`octocode-agent: Pi host (${PI_PACKAGE}) is not installed. Run: octocode-agent update`);
    return 1;
  }
  const coreRoot = (deps.resolveCoreRoot ?? resolveCoreRoot)();
  if (!coreRoot) {
    log(`octocode-agent: core (${CORE_PACKAGE}) is not installed. Run: octocode-agent update`);
    return 1;
  }
  const env = buildLaunchEnv(deps.env ?? process.env);
  // Spec is env-overridable (npm:/git:/path) so a packaging change is config, not code.
  const spec = env.OCTOCODE_AGENT_EXTENSION_SPEC ?? coreRoot;
  const result = run(piInfo.bin, buildPiArgs(spec, argv, env), { env }, spawn);
  return typeof result?.status === 'number' ? result.status : (result?.error ? 1 : 0);
}

/** npm install command for a self-update, split platform vs core-only. */
export function updateCommand(target) {
  if (target === 'core') {
    // BYO / in-place: refresh only the core dependency.
    return { cmd: 'npm', args: ['update', CORE_PACKAGE] };
  }
  // Self-update the whole platform globally (pins the newest core).
  return { cmd: 'npm', args: ['install', '-g', 'octocode-agent@latest'] };
}

export function runUpdate(target, deps = {}) {
  const spawn = deps.spawn ?? spawnSync;
  const log = deps.log ?? console.error;
  const { cmd, args } = updateCommand(target);
  log(`octocode-agent: ${target === 'core' ? 'updating core' : 'self-updating platform'} → ${cmd} ${args.join(' ')}`);
  const result = run(cmd, args, {}, spawn);
  return typeof result?.status === 'number' ? result.status : (result?.error ? 1 : 0);
}

export function main(argv = [], deps = {}) {
  const out = deps.out ?? console.log;
  const { command, target, rest } = parseInvocation(argv);
  switch (command) {
    case 'version':
      out(versionReport());
      return 0;
    case 'help':
      out(helpReport());
      return 0;
    case 'update':
      return runUpdate(target, deps);
    case 'run':
    default:
      return launchAgent(rest ?? [], deps);
  }
}
