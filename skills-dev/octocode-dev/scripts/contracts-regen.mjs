#!/usr/bin/env node
// `contracts:regen`: regenerate the tool contract from the installed core package, one run at a time.
//
// Two sessions that regenerate at once race on core's `dist/` (one deletes it while the
// other reads it), and a regeneration over a core that does not typecheck publishes a
// half-made contract to every agent in the tree. So the whole task holds the machine-wide
// regeneration lock (packages/octocode-native/scripts/build-slot.cjs, kind `regen`), and
// with a sibling core checked out it refuses unless core typechecks.
import { spawnSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import { createRequire } from 'node:module';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { ROOT } from './lib/workspaces.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const CORE = path.resolve(ROOT, '..', 'octocode-mcp-host', 'packages', 'octocode-core');
const { REGEN_KIND, exitOnSignals, withSlot } = createRequire(import.meta.url)(
  path.join(ROOT, 'packages', 'octocode-native', 'scripts', 'build-slot.cjs'),
);

/** The steps in order, as [what it does, command, args, cwd]; the first applies only beside a sibling core. */
export const STEPS = [
  ['core typecheck', 'yarn', ['typecheck'], CORE],
  ['core build', 'node', [path.join(HERE, 'core-build.mjs')], ROOT],
  ['install pinned dependencies', 'yarn', ['install'], ROOT],
  ['generate the tool contract', 'yarn', ['workspace', '@octocodeai/config', 'generate:tool-contract'], ROOT],
  ['tool-flow route table', 'node', [path.join(ROOT, 'skills', 'common', 'tool-flow.mjs')], ROOT],
];

function run(env) {
  for (const [what, command, args, cwd] of STEPS) {
    if (cwd === CORE && !existsSync(CORE)) continue;
    const { status, error } = spawnSync(command, args, { cwd, stdio: 'inherit', env });
    if (error) {
      console.error(`contracts:regen: ${what} did not start: ${error.message}`);
      return 1;
    }
    if (status !== 0) {
      const hint = what === 'core typecheck' ? ' (core does not typecheck: fix it, or wait for the agent editing it; nothing was regenerated)' : '';
      console.error(`contracts:regen: ${what} failed${hint}`);
      return status ?? 1;
    }
  }
  return 0;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  exitOnSignals();
  process.exit(await withSlot('contracts:regen', async slot => run(slot.env), { kind: REGEN_KIND, count: 1 }));
}
