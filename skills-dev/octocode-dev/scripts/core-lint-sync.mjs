#!/usr/bin/env node
// Core's lint setup is a copy of this repo's: ../octocode-mcp-host/packages/octocode-core keeps
// scripts/oxlint-baseline.mjs and .oxlintrc.json beside its own baseline. This repo owns both.
// Fails when the core copy drifts: the baseline script must be byte-identical, and the config
// must carry the same rules (only `$schema`, `ignorePatterns`, the CLI-flag `options` and overrides scoped to this repo's
// `packages/...` or `skills/...` paths are layout-specific). Skips with a note when the sibling checkout is absent.
// Usage: node skills-dev/octocode-dev/scripts/core-lint-sync.mjs
import { isDeepStrictEqual } from 'node:util';
import fs from 'node:fs';
import path from 'node:path';
import { isMainModule } from './lib/entrypoint.mjs';
import { ROOT } from './lib/workspaces.mjs';
import { parse } from './oxlint-baseline.mjs';

const CORE = path.resolve(ROOT, '..', 'octocode-mcp-host', 'packages', 'octocode-core');
const LAYOUT_KEYS = new Set(['$schema', 'ignorePatterns', 'options']);

/** An override is layout-specific when every glob names a path under this repo's `packages/` or `skills/`. */
const repoScoped = override => (override.files ?? []).every(glob => /^(packages|skills)\//.test(glob));

/** The rule-bearing part of an oxlint config: layout keys and repo-scoped overrides removed. */
export function ruleView(config) {
  const view = Object.fromEntries(Object.entries(config).filter(([key]) => !LAYOUT_KEYS.has(key)));
  if (view.overrides) view.overrides = view.overrides.filter(override => !repoScoped(override));
  return view;
}

/** Top-level keys whose rule-bearing value differs between the canonical config and the copy. */
export function configDrift(canonical, copy) {
  const [want, have] = [ruleView(canonical), ruleView(copy)];
  return [...new Set([...Object.keys(want), ...Object.keys(have)])]
    .filter(key => !isDeepStrictEqual(want[key], have[key]))
    .toSorted();
}

const readConfig = dir => parse(fs.readFileSync(path.join(dir, '.oxlintrc.json'), 'utf8'));

function main() {
  if (!fs.existsSync(CORE)) {
    console.log(`core-lint-sync: skipped, no sibling checkout at ${path.relative(ROOT, CORE)}`);
    return 0;
  }
  const problems = [];
  const script = ['skills-dev/octocode-dev/scripts/oxlint-baseline.mjs', 'scripts/oxlint-baseline.mjs'];
  if (fs.readFileSync(path.join(ROOT, script[0]), 'utf8') !== fs.readFileSync(path.join(CORE, script[1]), 'utf8')) {
    problems.push(`${script[1]} differs from ${script[0]}: copy the canonical file over it`);
  }
  const drift = configDrift(readConfig(ROOT), readConfig(CORE));
  if (drift.length) problems.push(`.oxlintrc.json rules differ from this repo's in: ${drift.join(', ')}`);
  for (const problem of problems) console.error(`core-lint-sync: octocode-core ${problem}`);
  if (!problems.length) console.log('core-lint-sync: octocode-core lint config and baseline script match');
  return problems.length ? 1 : 0;
}

if (isMainModule(import.meta.url)) process.exit(main());
