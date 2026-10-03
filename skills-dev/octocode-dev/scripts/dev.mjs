#!/usr/bin/env node
// Repo task runner for the Octocode monorepo; replaces the root package.json scripts.
// Usage: node skills-dev/octocode-dev/scripts/dev.mjs <task> [args...]   (--help lists tasks)
import { spawnSync } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(HERE, '..', '..', '..');
// CI never builds native: native artifacts are built locally (`build:all` / `build:publish`).
const CI_EXCLUDE = ['--exclude', '@octocodeai/octocode-benchmark', '--exclude', '@octocodeai/octocode-native'];

const script = (name, ...args) => ['node', [path.join(HERE, name), ...args]];
const health = (...args) => script('workspace-health.mjs', ...args);
const guard = ['node', [path.join(ROOT, 'packages/octocode/scripts/check-no-workspace-protocol.mjs')]];
const yarn = (...args) => ['yarn', args];

/** task → [description, steps]; extra CLI args go to the last step. */
const TASKS = {
  build: ['Release build of every workspace (slow)', [health('run', 'build', '--parallel')]],
  'build:dev': ['Fast debug build of every workspace (default locally)', [health('run', 'build', '--parallel', '--prefer', 'build:dev')]],
  'build:ci': ['Publish guard, then release build without benchmark or native', [script('prepublish.mjs'), guard, health('run', 'build', '--parallel', ...CI_EXCLUDE)]],
  'build:publish': ['Publish guard, 6-platform native build, platform check, MCP publish build', [script('prepublish.mjs'), guard, yarn('build:native:all'), yarn('platforms:check'), yarn('workspace', 'octocode-mcp', 'build:publish')]],
  test: ['Run every workspace test script', [health('run', 'test')]],
  'test:ci': ['Tests without benchmark or native', [health('run', 'test', ...CI_EXCLUDE)]],
  lint: ['Run every workspace lint script', [health('run', 'lint')]],
  'lint:ci': ['Lint without benchmark or native', [health('run', 'lint', ...CI_EXCLUDE)]],
  typecheck: ['Run every workspace typecheck script', [health('run', 'typecheck')]],
  'typecheck:ci': ['Typecheck without benchmark or native', [health('run', 'typecheck', ...CI_EXCLUDE)]],
  verify: ['Full repo contract: dedupe, docs, per-package verify', [health('verify')]],
  'health:check': ['Required workspace scripts exist', [health('check')]],
  'health:report': ['Workspace script matrix', [health('report')]],
  'check-outputs': ['Build outputs exist after a build', [health('check-outputs')]],
  'docs:verify': ['Docs links, catalog, config keys, publishing contracts', [script('docs-verify.mjs')]],
  'deps:dedupe': ['One version range per external dependency (--fix rewrites)', [script('dedupe-deps.mjs')]],
  setup: ['Local dev resolutions (--dry-run, --install, --reset); then yarn install', [script('dev-setup.mjs')]],
  prepublish: ['Publish guard (--fix strips local resolutions, --dry-run previews)', [script('prepublish.mjs')]],
  guard: ['Final npm-publish guard: no workspace:/file:/link: protocols ship', [guard]],
};

function help() {
  const width = Math.max(...Object.keys(TASKS).map(name => name.length));
  console.log('Usage: node skills-dev/octocode-dev/scripts/dev.mjs <task> [args...]\n\nTasks:');
  for (const [name, [description]] of Object.entries(TASKS)) {
    console.log(`  ${name.padEnd(width)}  ${description}`);
  }
}

const [task, ...extra] = process.argv.slice(2);
if (!task || task === '--help' || task === '-h') {
  help();
  process.exit(task ? 0 : 1);
}
if (!TASKS[task]) {
  console.error(`Unknown task "${task}".\n`);
  help();
  process.exit(1);
}

const steps = TASKS[task][1];
// A plain `prepublish` check also runs the final guard (the old root `prepublish`);
// flagged runs (--fix, --dry-run) only rewrite or preview manifests.
const plan = task === 'prepublish' && extra.length === 0 ? [...steps, guard] : steps;
plan.forEach(([command, args], index) => {
  const finalArgs = index === steps.length - 1 ? [...args, ...extra] : args;
  const { status, error } = spawnSync(command, finalArgs, { cwd: ROOT, stdio: 'inherit' });
  if (error) {
    console.error(`dev ${task}: ${command} failed to start: ${error.message}`);
    process.exit(1);
  }
  if (status !== 0) process.exit(status ?? 1);
});
