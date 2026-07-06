// octocode-agent launcher core.
//
// Architecture:
//   - @octocodeai/pi-extension is THE CORE (system prompt, skills, tools, awareness).
//   - octocode-agent is the PLATFORM: bundles the Pi binary and launches it with the
//     core loaded per-run via pi's -e flag. The core is a real dependency so npx and
//     global installs carry the same pinned harness.
//
// Core resolution (resolveCoreSpec):
//   1. OCTOCODE_AGENT_EXTENSION_SPEC env (explicit override)
//   2. Installed @octocodeai/pi-extension dependency (npx/global/local fast path)
//   3. npm:@octocodeai/pi-extension — pi fetches from npm for this run (recovery fallback)
//
// The npm: fallback is a recovery path only; normal installs resolve the bundled dep.
//
// Fork wiring — env overrides (no code change required when switching to a fork):
//   OCTOCODE_PI_BIN      Absolute path to a locally-built Pi binary. Takes priority
//                        over package resolution. Used during fork dev workflow.
//   OCTOCODE_PI_PACKAGE  npm package name override (e.g. @octocodeai/pi-coding-agent).
//                        Used when the fork is published under a different name.
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
// npm: spec for pi's -e flag — pi downloads to a temp dir for the current run.
// Used when @octocodeai/pi-extension is not installed locally or globally.
export const CORE_SPEC = `npm:${CORE_PACKAGE}`;

// Canonical upstream Pi package. Override with OCTOCODE_PI_PACKAGE env var to use a fork.
export const PI_PACKAGE = '@earendil-works/pi-coding-agent';

/** The effective Pi package name — reads OCTOCODE_PI_PACKAGE override first. */
export function getEffectivePiPackage(env = process.env) {
  return env.OCTOCODE_PI_PACKAGE || PI_PACKAGE;
}

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
  // the package's `exports` map — Pi's "." export defines only `import` (ESM), which a
  // CJS require.resolve of the entry cannot satisfy.
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

/**
 * Resolve the spec to pass to pi's -e flag for loading the core extension.
 *
 * Resolution order:
 *   1. OCTOCODE_AGENT_EXTENSION_SPEC env var (explicit override)
 *   2. Local path from resolvePackageJson (works for local and global npm installs)
 *   3. CORE_SPEC (npm:@octocodeai/pi-extension) — pi downloads for this run
 *
 * Always returns a non-null spec. Normal npx/global installs resolve the bundled
 * dependency; the npm: fallback is only for damaged/dev installs where the core is absent.
 */
export function resolveCoreSpec(env = process.env) {
  if (env.OCTOCODE_AGENT_EXTENSION_SPEC) return env.OCTOCODE_AGENT_EXTENSION_SPEC;
  // Fast path: bundled/local/global dependency installed next to this launcher.
  const pkgJson = resolvePackageJson(CORE_PACKAGE);
  if (pkgJson) return path.dirname(pkgJson);
  // Recovery fallback: pi fetches from npm for this run (no local install needed).
  return CORE_SPEC;
}

/**
 * Resolve the Pi host executable. Resolution order:
 *   1. OCTOCODE_PI_BIN env var — absolute path to a locally-built Pi binary (fork dev).
 *   2. OCTOCODE_PI_PACKAGE env var — resolve bin from an alternate npm package (fork prod).
 *   3. Default PI_PACKAGE (@earendil-works/pi-coding-agent) — the bundled upstream release.
 *
 * Returns { bin, pkgRoot, source } or null.
 * `source` is 'env-bin' | 'env-package' | 'bundled' for diagnostics.
 */
export function resolvePiBin(env = process.env) {
  // 1. Direct binary override — local fork dev workflow.
  if (env.OCTOCODE_PI_BIN) {
    const bin = env.OCTOCODE_PI_BIN;
    if (!fs.existsSync(bin)) return null;
    return { bin, pkgRoot: path.dirname(bin), source: 'env-bin' };
  }

  // 2. Package-name override (fork published to npm) or bundled default.
  const pkgName = getEffectivePiPackage(env);
  const source = env.OCTOCODE_PI_PACKAGE ? 'env-package' : 'bundled';
  const pkgJson = resolvePackageJson(pkgName);
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
  return fs.existsSync(bin) ? { bin, pkgRoot, source } : null;
}

/**
 * Build the environment Pi launches with.
 * - OCTOCODE_PROMPT_MODE=octocode-first: the core leads the system prompt (branded agent).
 * - OCTOCODE_AGENT=1: a marker the core / hooks can key off.
 * Never clobbers a value the user explicitly set.
 */
export function buildLaunchEnv(baseEnv = process.env) {
  const env = { ...baseEnv };
  if (!env.OCTOCODE_PROMPT_MODE) env.OCTOCODE_PROMPT_MODE = 'octocode-first';
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

export function versionReport(env = process.env) {
  const effectivePkg = getEffectivePiPackage(env);
  const piVersion = env.OCTOCODE_PI_BIN
    ? `(local binary: ${env.OCTOCODE_PI_BIN})`
    : (readPackageVersion(effectivePkg) ?? 'not installed');

  const coreVersion = readPackageVersion(CORE_PACKAGE);
  const coreStatus = coreVersion
    ? coreVersion
    : `not installed locally — will use ${CORE_SPEC} on run`;

  const lines = [
    `octocode-agent   ${launcherVersion() ?? '?'}`,
    `core (${CORE_PACKAGE})   ${coreStatus}`,
    `pi host (${effectivePkg})   ${piVersion}`,
  ];
  return lines.join('\n');
}

export function helpReport() {
  return [
    'octocode-agent — self-working coding agent (Pi + Octocode harness core)',
    '',
    'Usage:',
    '  octocode-agent [pi args...]   Launch the agent (forwards args to the Pi host)',
    '  octocode-agent update         Self-update the platform',
    '  octocode-agent update core    Update the bundled core extension in this install',
    '  octocode-agent --version      Print launcher, core, and Pi host versions',
    '',
    `The core (${CORE_PACKAGE}) carries the prompt, skills, tools, and memory.`,
    'The core is installed as a platform dependency. Refresh it without reinstalling Pi:',
    '  octocode-agent update core',
    '',
    'If the local dependency is missing, the core is fetched from npm per run as a recovery fallback.',
    '',
    'Fork dev env vars (no code change required):',
    '  OCTOCODE_PI_BIN      Absolute path to a locally-built Pi binary',
    '  OCTOCODE_PI_PACKAGE  npm package name override (e.g. @octocodeai/pi-coding-agent)',
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
 * rules, so a single flag brings the extension AND the bundled skills. `-e npm:` lets
 * pi resolve and download the package from npm directly (lean mode).
 *
 * Fixed flags (always on):
 *   --no-extensions      Prevents globally-installed extensions from conflicting with
 *                        the loaded core. Extension-registered tools stay active.
 * Context files (AGENTS.md / CLAUDE.md) stay enabled by default so repository rules
 * remain authoritative. Disable them only for deterministic harness-only runs.
 *
 * Opt-in / opt-out flags:
 *   OCTOCODE_AGENT_CLEAN=1              also passes --no-skills and --no-context-files
 *   OCTOCODE_AGENT_FULL_TOOLS=1         keeps grep/find/ls (opt out of lean set)
 *   OCTOCODE_AGENT_NO_CONTEXT_FILES=1   suppresses AGENTS.md / CLAUDE.md loading
 */
export function buildPiArgs(spec, argv = [], env = {}) {
  const args = ['--no-extensions'];
  const clean = env.OCTOCODE_AGENT_CLEAN === '1';
  if (clean) args.push('--no-skills');
  if (env.OCTOCODE_AGENT_FULL_TOOLS !== '1') args.push('--exclude-tools', LEAN_EXCLUDE_TOOLS.join(','));
  if (clean || env.OCTOCODE_AGENT_NO_CONTEXT_FILES === '1') args.push('--no-context-files');
  args.push('-e', spec);
  return [...args, ...argv];
}

/**
 * Launch the Pi host with the core loaded per-run.
 * The core spec is resolved via resolveCoreSpec — local install (fast) or npm: (lean).
 * Only the Pi binary is required to be installed; the core is optional locally.
 */
export function launchAgent(argv = [], deps = {}) {
  const spawn = deps.spawn ?? spawnSync;
  const log = deps.log ?? console.error;
  const env = buildLaunchEnv(deps.env ?? process.env);

  const piInfo = (deps.resolvePiBin ?? resolvePiBin)(env);
  if (!piInfo) {
    const effectivePkg = getEffectivePiPackage(env);
    const hint = env.OCTOCODE_PI_BIN
      ? `OCTOCODE_PI_BIN path not found: ${env.OCTOCODE_PI_BIN}`
      : `Pi host (${effectivePkg}) is not installed. Run: octocode-agent update`;
    log(`octocode-agent: ${hint}`);
    return 1;
  }

  // Core spec: local install (fast) or npm: fallback (pi fetches on demand).
  const spec = (deps.resolveCoreSpec ?? resolveCoreSpec)(env);
  const result = run(piInfo.bin, buildPiArgs(spec, argv, env), { env }, spawn);
  return typeof result?.status === 'number' ? result.status : (result?.error ? 1 : 0);
}

/**
 * npm install command for updates.
 * Core is a platform dependency. Updating it installs the newest extension into this
 * launcher package root, which works for npx cache installs, local dev, and global installs.
 */
export function launcherRoot() {
  return path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
}

export function updateCommand(target, options = {}) {
  if (target === 'core') {
    const prefix = options.prefix ?? launcherRoot();
    return { cmd: 'npm', args: ['install', '--prefix', prefix, '--omit=dev', `${CORE_PACKAGE}@latest`] };
  }
  // Self-update the whole platform globally. For npx, run `npx -y octocode-agent@latest`.
  return { cmd: 'npm', args: ['install', '-g', 'octocode-agent@latest'] };
}

export function runUpdate(target, deps = {}) {
  const spawn = deps.spawn ?? spawnSync;
  const log = deps.log ?? console.error;
  const { cmd, args } = updateCommand(target, deps);
  log(`octocode-agent: ${target === 'core' ? 'updating core' : 'self-updating platform'} → ${cmd} ${args.join(' ')}`);
  const result = run(cmd, args, {}, spawn);
  return typeof result?.status === 'number' ? result.status : (result?.error ? 1 : 0);
}

export function main(argv = [], deps = {}) {
  const out = deps.out ?? console.log;
  const env = deps.env ?? process.env;
  const { command, target, rest } = parseInvocation(argv);
  switch (command) {
    case 'version':
      out(versionReport(env));
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
