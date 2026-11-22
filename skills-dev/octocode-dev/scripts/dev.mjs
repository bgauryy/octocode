#!/usr/bin/env node
// Repo task runner for the Octocode monorepo; replaces the root package.json scripts.
// Usage: node skills-dev/octocode-dev/scripts/dev.mjs <task> [args...]   (--help lists tasks)
import { spawnSync } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { ROOT, SKILL_ROOTS } from './lib/workspaces.mjs';
import { skillTestCommand } from './lib/skill-tests.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const NATIVE = path.join(ROOT, 'packages', 'octocode-native');
// CI never builds native: native artifacts are built locally (`build:all` / `build:publish`).
const CI_EXCLUDE = ['--exclude', '@octocodeai/octocode-native'];

const script = (name, ...args) => ['node', [path.join(HERE, name), ...args]];
const health = (...args) => script('workspace-health.mjs', ...args);
const yarn = (...args) => ['yarn', args];
// A step that builds Rust holds a machine-wide build slot for its whole run (packages/octocode-native/scripts/build-slot.cjs).
const slotted = ([command, args], options) => ['node', [path.join(NATIVE, 'scripts', 'build-slot.cjs'), '--', command, ...args], options];
// Shared assets have a private workspace; skill folders remain independently installable.
const skill = (rel, ...args) => ['node', [path.join(ROOT, 'skills', rel), ...args]];
const skillChecks = () => [
  skill('octocode-skills/scripts/skill-review.mjs', '--collections', ...SKILL_ROOTS.map(dir => path.join(ROOT, dir))),
  skill('octocode-skills/scripts/skill-review.mjs', '--self-test'),
  skill('octocode-skills/scripts/skill-sync.mjs', '--self-test'),
  skillTestCommand(ROOT),
];
// Shrink-only ratchet: the oxlint violations that predate a rule (the one repo-wide oxlint run).
const OXLINT_BASELINE = script('oxlint-baseline.mjs');
// The shared skill workspace's own lint/test; test:ci and lint:ci reach them through the workspace run.
const SKILLS_COMMON = [yarn('workspace', '@octocodeai/skills-common', 'lint'), yarn('workspace', '@octocodeai/skills-common', 'test')];

/** task → [description, steps]; a step is [command, args, spawn options?]; extra CLI args go to the last step. */
const TASKS = {
  build: ['Release-mode build of every workspace (native host only, slow)', [health('run', 'build', '--parallel')]],
  'build:dev': ['Fast debug build of every workspace (native host only, default locally); shared cache cleanup is a separate explicit task', [health('run', 'build', '--parallel', '--prefer', 'build:dev')]],
  'build:ci': ['Resolution check, then release build without native', [script('prepublish.mjs'), health('run', 'build', '--parallel', ...CI_EXCLUDE)]],
  'build:publish': ['Resolution check, all-platform native build, MCP publish build', [script('prepublish.mjs'), yarn('workspace', '@octocodeai/octocode-native', 'build:all'), yarn('workspace', 'octocode-mcp', 'build')]],
  test: ['Run every workspace test script', [health('run', 'test')]],
  'test:ci': ['Skill checks, the release build without native (the tests import dist/), then tests in parallel without native, each under its coverage floors when it declares test:coverage', () => [...skillChecks(), health('run', 'build', '--parallel', ...CI_EXCLUDE), health('run', 'test', '--parallel', '--prefer', 'test:coverage', ...CI_EXCLUDE)]],
  'test:dev': ['Focused Rust tests: the test:rust build, then only the test binaries asked for, side by side, in a build slot ([--crate tool-github]... [--lib] [--] [libtest filter, e.g. local_search::])', [['node', [path.join(NATIVE, 'scripts', 'rust-tests.cjs'), 'dev'], { cwd: NATIVE }]]],
  'skills:check': ['Skill lint, shared report viewer, and skill script tests', () => [...skillChecks(), ...SKILLS_COMMON]],
  'answers:record': ['Re-record the provider answers native mocks replay, live, read-only and public (test-utils/provider-answers; needs build:dev; [id,...])', [script('providers/record.mjs')]],
  check: ['Fast local static check: oxlint baseline, rustfmt, the clippy ratchet, every other workspace lint:own and typecheck, in parallel. No build, no tests: pair it with a focused test', [OXLINT_BASELINE, yarn('workspace', '@octocodeai/octocode-native', 'fmt:rust:check'), script('rust-lint.mjs'), health('run', 'lint:own', '--parallel', '--exclude', '@octocodeai/octocode-native'), health('run', 'typecheck', '--parallel')]],
  slots: ['Who holds each machine-wide Rust build slot now (pid, lane, command, time held)', [['node', [path.join(NATIVE, 'scripts', 'build-slot.cjs'), '--status']]]],
  // The whole release gate set as a dependency graph with one summary table (gates.mjs).
  gates: ['Release gate set in one run: build:dev, then check, contract check, config tests, docs:verify, skills:check and test:rust side by side (cargo queues on the build slots), then the sibling core tests and typecheck; one summary table, exit 1 on any failure', [script('gates.mjs')]],
  'lint:rust': ['Local clippy ratchet: plain clippy plus production-only bans and lints, whose old findings may only shrink ([--write], [--adopt])', [script('rust-lint.mjs')]],
  lint: ['Oxlint baseline (the one oxlint run), then every workspace lint:own script', [OXLINT_BASELINE, health('run', 'lint:own', '--parallel')]],
  'lint:ci': ['Lint without native workspace scripts (the oxlint baseline still covers them)', [OXLINT_BASELINE, health('run', 'lint:own', ...CI_EXCLUDE)]],
  format: ['Fix the formatting lint checks: every workspace `format` script (prettier --write)', [yarn('workspaces', 'foreach', '-Apt', 'run', 'format')]],
  typecheck: ['Run every workspace typecheck script', [health('run', 'typecheck', '--parallel')]],
  'typecheck:ci': ['Typecheck without native', [health('run', 'typecheck', ...CI_EXCLUDE)]],
  verify: ['Full repo contract: oxlint baseline (the one oxlint run), core lint-copy sync, dedupe, docs, per-package verify:own', [OXLINT_BASELINE, script('core-lint-sync.mjs'), health('verify')]],
  'health:check': ['Required workspace scripts exist', [health('check')]],
  'health:report': ['Workspace script matrix', [health('report')]],
  'check-outputs': ['Build outputs and host native version/contract match (--all checks every staged platform)', [health('check-outputs')]],
  clean: ['Remove every workspace build output (dist/, out/, native target/ + staged binaries)', [yarn('workspaces', 'foreach', '-A', 'run', 'clean')]],
  'clean:cache': ['Drop stale native build copies; compiled dependencies stay warm', [yarn('workspace', '@octocodeai/octocode-native', 'clean:cache')]],
  'prune:rust': ['Delete native target/ outputs no build:dev, test:rust or lint:rust run of the last 7 days used, and copied binaries; refuses while cargo runs ([--dry-run] [--days N])', [['node', [path.join(NATIVE, 'scripts', 'target-hygiene.cjs'), 'prune']]]],
  'docs:verify': ['Docs links, catalog, config keys, publishing contracts', [script('docs-verify.mjs')]],
  // Every intra-doc link in the native crates resolves; private items are documented, so links to them count too.
  'docs:rust': ['Native rustdoc with broken intra-doc links denied (cargo doc --no-deps --document-private-items, in a build slot)', () => [slotted(['cargo', ['doc', '--locked', '--no-deps', '--document-private-items', '--workspace']], { cwd: NATIVE, env: { ...process.env, RUSTDOCFLAGS: [process.env.RUSTDOCFLAGS, '-D rustdoc::broken_intra_doc_links'].filter(Boolean).join(' ') } })]],
  'deps:dedupe': ['One version range per external dependency (--fix rewrites)', [script('dedupe-deps.mjs')]],
  'contracts:regen': ['Regenerate the tool contract, one run at a time machine-wide: core typecheck (refuses when it fails), core build, install pinned dependencies, generate the contract, then the tool-flow route table', [script('contracts-regen.mjs')]],
  setup: ['Use sibling core locally (--reset for registry, --dry-run to preview, --install to install)', [script('dev-setup.mjs')]],
  prepublish: ['Local-resolution check (--fix strips local resolutions, --dry-run previews)', [script('prepublish.mjs')]],
  'brew:formula': ['Render the Homebrew formula from the published native npm packages ([version] [--out FILE])', [script('brew-formula.mjs')]],
  'lsp:lock': ['Generate the managed language-server lock from GitHub release digests ([--check] diffs it, [--bump] moves to the latest releases)', [script('lsp-lock.mjs')]],
  // Opt-in Rust quality tools, pinned in packages/octocode-native/dev-tools.json; none runs in CI or installs anything.
  'rust:tools': ['Pinned Rust dev tools vs installed; prints install commands ([--json])', [script('rust-quality.mjs', 'tools')]],
  'fuzz:smoke': ['Every libFuzzer target for 60 s on the pinned nightly ([--seconds N] [target...])', [slotted(script('rust-quality.mjs', 'fuzz', '--smoke'))]],
  fuzz: ['A longer libFuzzer campaign (600 s per target; [--seconds N] [target...])', [slotted(script('rust-quality.mjs', 'fuzz'))]],
  miri: ['The selected Miri tests on the pinned nightly, with a deadline', [slotted(script('rust-quality.mjs', 'miri'))]],
  mutants: ['Mutation test of chosen code, never per commit (-f <file>, --in-diff <diff>, -p <package>; slow: copies the tree and builds it)', [yarn('workspace', '@octocodeai/octocode-native', 'mutants')]],
  profile: ['Profile one tool flow: instructions, peak memory, flamegraph, top frames (<tool> <query json> [--phase first|repeat|page:N|one] [--cwd DIR]; --heap, --build)', [['python3', ['-I', '-B', path.join(HERE, 'profile', 'profile.py')]]]],
  'llvm-lines': ['LLVM IR lines per generic instantiation of one crate ([package] [--timings])', [slotted(script('rust-quality.mjs', 'llvm-lines'))]],
};

function help() {
  const width = Math.max(...Object.keys(TASKS).map(name => name.length));
  console.log('Usage: node skills-dev/octocode-dev/scripts/dev.mjs <task> [args...]\n\nTasks:');
  for (const [name, [description]] of Object.entries(TASKS)) {
    console.log(`  ${name.padEnd(width)}  ${String(description)}`);
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

let steps;
try {
  const selected = TASKS[task][1];
  steps = typeof selected === 'function' ? selected() : selected;
} catch (error) {
  console.error(`dev ${task}: ${error.message}`);
  process.exit(1);
}
steps.forEach(([command, args, options], index) => {
  const finalArgs = index === steps.length - 1 ? [...args, ...extra] : args;
  const { status, error } = spawnSync(command, finalArgs, { cwd: ROOT, stdio: 'inherit', ...options });
  if (error) {
    console.error(`dev ${task}: ${command} failed to start: ${error.message}`);
    process.exit(1);
  }
  if (status !== 0) process.exit(status ?? 1);
});
