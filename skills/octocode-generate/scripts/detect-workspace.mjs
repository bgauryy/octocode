#!/usr/bin/env node
// Read-only facts about a target folder before scaffolding: languages, package manager, monorepo, existing tooling.
import { existsSync, statSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { dirname, join, resolve } from 'node:path';
import { fail, errorMessage } from './lib/fail.mjs';
import { isMainModule } from './lib/entrypoint.mjs';
import { entries, readJson, readText, stacksIn } from './lib/stacks.mjs';

const HELP = `Usage: node <skill-folder>/scripts/detect-workspace.mjs <target-dir> [--no-probe]

Reports what already exists at and above the target folder, so the plan follows it instead of guessing.
The target may not exist yet (a new project inside an existing repository or monorepo).

Options:
  --no-probe   Skip running '<tool> --version' for the toolchains on PATH
  --help       Show this help

Output: one JSON object on stdout.
  target          {path, exists, empty}
  root            nearest ancestor (or the target) holding a lockfile, packageManager or workspace manifest
                  (JS workspaces, Cargo, go.work, Gradle settings, .NET solution, Dart or Deno workspace); null if none
  gitRoot         nearest ancestor with .git, or null
  languages       stacks found at target or root: javascript, typescript, deno, rust, python, go, dart,
                  dotnet, jvm, swift, ruby, php, elixir
  packageManager  {name, version, source}: source is packageManager, devEngines or a lockfile name; null if none
  lockfiles       every JS lockfile at the root; more than one is a conflict to resolve with the user
  monorepo        [{kind, file}]: npm/yarn/bun workspaces, pnpm-workspace, turbo, nx, cargo, go.work, uv,
                  gradle, dotnet solution, dart and deno workspaces
  tools           existing config per tool (eslint, eslintLegacy, prettier, biome, oxlint, oxfmt, vitePlus,
                  vitest, jest, playwright, typescript, hooks, rust*, python*, golangci, dartAnalysis,
                  swiftlint, swiftFormat, rubocop, pint, phpstan, credo, detekt, dotnetSdk): file names found
  scripts         package.json scripts at the target (or root)
  node            {engines, pinned}: engines.node and .nvmrc/.node-version content
  ci              GitHub workflow files at the root
  probes          {tool: version or null} for node, npm, pnpm, yarn, bun, deno, cargo, rustc, uv, python3,
                  go, flutter, dart, dotnet, java, swift, xcodebuild, ruby, php, elixir
Effects: reads files; unless --no-probe, runs each toolchain's version command (10 s limit each); writes nothing.
Exit codes: 0 report printed; 2 invalid input, as {"ok":false,"error":{code,message}}.`;

const LOCKFILES = { 'pnpm-lock.yaml': 'pnpm', 'yarn.lock': 'yarn', 'bun.lock': 'bun', 'bun.lockb': 'bun', 'package-lock.json': 'npm', 'npm-shrinkwrap.json': 'npm' };
// Version command per toolchain: most take --version; go and xcodebuild do not.
const PROBES = { node: ['--version'], npm: ['--version'], pnpm: ['--version'], yarn: ['--version'], bun: ['--version'], deno: ['--version'],
  cargo: ['--version'], rustc: ['--version'], uv: ['--version'], python3: ['--version'], go: ['version'], flutter: ['--version'], dart: ['--version'],
  dotnet: ['--version'], java: ['-version'], swift: ['--version'], xcodebuild: ['-version'], ruby: ['--version'], php: ['--version'], elixir: ['--version'] };
const TOOL_FILES = {
  eslint: /^eslint\.config\.(js|mjs|cjs|ts|mts|cts)$/, eslintLegacy: /^\.eslintrc(\.(js|cjs|json|ya?ml))?$/,
  prettier: /^(\.prettierrc(\.(json|ya?ml|js|cjs|mjs|toml))?|prettier\.config\.(js|cjs|mjs|ts))$/,
  biome: /^biome\.jsonc?$/, oxlint: /^(\.oxlintrc\.json|oxlint\.config\.ts)$/, oxfmt: /^\.oxfmtrc\.jsonc?$/,
  vitest: /^vitest\.(config|workspace)\.(js|mjs|ts|mts)$/, jest: /^jest\.config\.(js|cjs|mjs|ts|json)$/,
  playwright: /^playwright\.config\.(js|ts|mjs)$/, typescript: /^tsconfig(\.[\w-]+)?\.json$/,
  hooks: /^(lefthook\.ya?ml|\.husky|\.lintstagedrc(\.\w+)?|lint-staged\.config\.\w+|\.pre-commit-config\.yaml)$/,
  rustToolchain: /^rust-toolchain(\.toml)?$/, rustfmt: /^\.?rustfmt\.toml$/, clippy: /^\.?clippy\.toml$/, cargoDeny: /^deny\.toml$/,
  ruff: /^\.?ruff\.toml$/, mypy: /^(mypy\.ini|\.mypy\.ini)$/, pytest: /^(pytest\.ini|conftest\.py)$/, golangci: /^\.golangci\.(ya?ml|toml|json)$/,
  dartAnalysis: /^analysis_options\.yaml$/, swiftlint: /^\.swiftlint\.ya?ml$/, swiftFormat: /^\.swift-format$/, rubocop: /^\.rubocop\.ya?ml$/,
  pint: /^pint\.json$/, phpstan: /^phpstan\.neon(\.dist)?$/, credo: /^\.credo\.exs$/, detekt: /^detekt\.ya?ml$/, dotnetSdk: /^global\.json$/,
};
const DEP_TOOLS = { vitePlus: 'vite-plus', vitest: 'vitest', jest: 'jest', playwright: '@playwright/test', typescript: 'typescript',
  eslint: 'eslint', prettier: 'prettier', biome: '@biomejs/biome', oxlint: 'oxlint', oxfmt: 'oxfmt' };

const WORKSPACE_FILES = ['pnpm-workspace.yaml', 'go.work', 'settings.gradle', 'settings.gradle.kts'];

function isRoot(dir) {
  const names = entries(dir);
  if (names.some(name => name in LOCKFILES || WORKSPACE_FILES.includes(name) || /\.slnx?$/.test(name))) return true;
  const pkg = names.includes('package.json') ? readJson(join(dir, 'package.json')) : null;
  if (pkg?.packageManager || pkg?.workspaces || pkg?.devEngines?.packageManager) return true;
  if (/^workspace:/m.test(readText(join(dir, 'pubspec.yaml'))) || /"workspace"\s*:/.test(readText(join(dir, 'deno.json')) || readText(join(dir, 'deno.jsonc')))) return true;
  return names.includes('Cargo.toml') && /^\[workspace\]/m.test(readText(join(dir, 'Cargo.toml')));
}

function ancestors(start) {
  const list = [];
  for (let dir = start; ; dir = dirname(dir)) {
    list.push(dir);
    if (dirname(dir) === dir) return list;
  }
}

function packageManagerOf(root) {
  if (!root) return { packageManager: null, lockfiles: [] };
  const lockfiles = entries(root).filter(name => name in LOCKFILES);
  const pkg = readJson(join(root, 'package.json'));
  const field = /^(npm|pnpm|yarn|bun)@([^+\s]+)/.exec(pkg?.packageManager ?? '');
  if (field) return { packageManager: { name: field[1], version: field[2], source: 'packageManager' }, lockfiles };
  const engine = pkg?.devEngines?.packageManager;
  const declared = Array.isArray(engine) ? engine[0] : engine;
  if (declared?.name) return { packageManager: { name: declared.name, version: declared.version ?? null, source: 'devEngines' }, lockfiles };
  if (lockfiles.length) return { packageManager: { name: LOCKFILES[lockfiles[0]], version: null, source: lockfiles[0] }, lockfiles };
  return { packageManager: null, lockfiles };
}

function monorepoOf(root) {
  if (!root) return [];
  const found = [];
  const pkg = readJson(join(root, 'package.json'));
  if (pkg?.workspaces) found.push({ kind: 'package.json workspaces', file: 'package.json' });
  const checks = [['pnpm-workspace', 'pnpm-workspace.yaml'], ['turbo', 'turbo.json'], ['nx', 'nx.json'], ['go.work', 'go.work']];
  for (const [kind, file] of checks) if (existsSync(join(root, file))) found.push({ kind, file });
  if (/^\[workspace\]/m.test(readText(join(root, 'Cargo.toml')))) found.push({ kind: 'cargo', file: 'Cargo.toml' });
  if (/^\[tool\.uv\.workspace\]/m.test(readText(join(root, 'pyproject.toml')))) found.push({ kind: 'uv', file: 'pyproject.toml' });
  for (const file of ['settings.gradle', 'settings.gradle.kts']) if (/^\s*include\b/m.test(readText(join(root, file)))) found.push({ kind: 'gradle', file });
  for (const file of entries(root).filter(name => /\.slnx?$/.test(name))) found.push({ kind: 'dotnet solution', file });
  if (/^workspace:/m.test(readText(join(root, 'pubspec.yaml')))) found.push({ kind: 'dart workspace', file: 'pubspec.yaml' });
  for (const file of ['deno.json', 'deno.jsonc']) if (/"workspace"\s*:/.test(readText(join(root, file)))) found.push({ kind: 'deno workspace', file });
  return found;
}

function toolsOf(dirs) {
  const tools = {};
  const add = (tool, value) => {
    tools[tool] ??= [];
    if (!tools[tool].includes(value)) tools[tool].push(value);
  };
  for (const dir of dirs) {
    for (const name of entries(dir)) for (const [tool, pattern] of Object.entries(TOOL_FILES)) if (pattern.test(name)) add(tool, name);
    const pkg = readJson(join(dir, 'package.json'));
    const deps = { ...pkg?.dependencies, ...pkg?.devDependencies };
    for (const [tool, dep] of Object.entries(DEP_TOOLS)) if (deps[dep]) add(tool, `${dep}@${deps[dep]}`);
    if (pkg?.prettier) add('prettier', 'package.json#prettier');
    if (deps['lint-staged'] || deps.husky || deps.lefthook) add('hooks', ['lint-staged', 'husky', 'lefthook'].filter(dep => deps[dep]).join(','));
    const pyproject = readText(join(dir, 'pyproject.toml'));
    for (const tool of ['ruff', 'mypy', 'pyright', 'ty', 'pytest']) if (new RegExp(`^\\[tool\\.${tool}(\\.|\\])`, 'm').test(pyproject)) add(tool, 'pyproject.toml');
    if (/^\[lints\]|^\[workspace\.lints\]/m.test(readText(join(dir, 'Cargo.toml')))) add('clippy', 'Cargo.toml#lints');
  }
  return tools;
}

function languagesOf(dirs, tools) {
  const found = new Set(dirs.flatMap(stacksIn));
  if (tools.typescript) found.add('typescript');
  return [...found];
}

function probe(tool, cwd) {
  const result = spawnSync(tool, PROBES[tool], { cwd, encoding: 'utf8', timeout: 10_000, stdio: ['ignore', 'pipe', 'pipe'],
    env: { ...process.env, COREPACK_ENABLE_DOWNLOAD_PROMPT: '0' } });
  if (result.error || result.status !== 0) return null;
  return (result.stdout || result.stderr).trim().split('\n')[0] || null;
}

/** The workspace report for target; probe=false skips running tools. */
export function detectWorkspace(target, { probe: runProbes = true } = {}) {
  const path = resolve(target);
  const exists = existsSync(path) && statSync(path).isDirectory();
  const chain = ancestors(path).filter(dir => existsSync(dir));
  const gitRoot = chain.find(dir => existsSync(join(dir, '.git'))) ?? null;
  const limit = gitRoot ? chain.indexOf(gitRoot) + 1 : chain.length;
  const root = chain.slice(0, limit).find(isRoot) ?? null;
  const dirs = [...new Set([exists ? path : null, root].filter(Boolean))];
  const tools = toolsOf(dirs);
  const { packageManager, lockfiles } = packageManagerOf(root);
  const manifestDir = dirs.find(dir => existsSync(join(dir, 'package.json')));
  const pkg = manifestDir ? readJson(join(manifestDir, 'package.json')) : null;
  const pinDir = dirs.find(dir => existsSync(join(dir, '.nvmrc')) || existsSync(join(dir, '.node-version')));
  const probeDir = exists ? path : chain[0];
  return {
    target: { path, exists, empty: exists ? entries(path).every(name => name === '.git') : true },
    root, gitRoot,
    languages: languagesOf(dirs, tools),
    packageManager, lockfiles,
    monorepo: monorepoOf(root),
    tools,
    scripts: pkg?.scripts ?? {},
    node: { engines: pkg?.engines?.node ?? null, pinned: pinDir ? (readText(join(pinDir, '.nvmrc')) || readText(join(pinDir, '.node-version'))).trim() : null },
    ci: root || gitRoot ? entries(join(root ?? gitRoot, '.github', 'workflows')).filter(name => /\.ya?ml$/.test(name)) : [],
    probes: runProbes ? Object.fromEntries(Object.keys(PROBES).map(tool => [tool, probe(tool, probeDir)])) : null,
  };
}

if (isMainModule(import.meta.url)) {
  const argv = process.argv.slice(2);
  if (argv.includes('--help') || argv.includes('-h')) { console.log(HELP); process.exit(0); }
  const unknown = argv.find(arg => arg.startsWith('--') && arg !== '--no-probe');
  const targets = argv.filter(arg => !arg.startsWith('--'));
  if (unknown) fail('usage', `unknown option ${unknown}`, 2);
  if (targets.length !== 1) fail('usage', 'give exactly one target directory', 2);
  try {
    console.log(JSON.stringify(detectWorkspace(targets[0], { probe: !argv.includes('--no-probe') }), null, 2));
  } catch (error) {
    fail('unreadable', errorMessage(error), 1);
  }
}
