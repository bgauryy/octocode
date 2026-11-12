// graphQuery `cochange`: a Git overlay read at query time. Its own scope is a
// small repository whose history fixes every expected count: `a.ts` changes in
// four commits, with `b.ts` (which imports it) in two and `c.ts` (no edge) in one.
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { firstDiff, parityView } from './compare.mjs';

const SCOPE = 'graph-cochange';
const GIT_ENV = {
  GIT_CONFIG_NOSYSTEM: '1', GIT_CONFIG_GLOBAL: '/dev/null',
  GIT_AUTHOR_NAME: 'Octo Dev', GIT_AUTHOR_EMAIL: 'dev@example.invalid',
  GIT_COMMITTER_NAME: 'Octo Dev', GIT_COMMITTER_EMAIL: 'dev@example.invalid',
};
const COMMITS = [
  { 'a.ts': 'export const a = 1;\n', 'b.ts': "import { a } from './a';\nexport const b = a;\n" },
  { 'a.ts': 'export const a = 2;\n', 'b.ts': "import { a } from './a';\nexport const b = a + 1;\n" },
  { 'a.ts': 'export const a = 3;\n', 'c.ts': 'export const c = 1;\n' },
  { 'a.ts': 'export const a = 4;\n' },
];

/** The repository under `<cwd>/graph-cochange`; a failure reason, or null. */
export function prepareGraphCochange(cwd) {
  const root = path.join(cwd, SCOPE);
  fs.mkdirSync(root, { recursive: true });
  const git = args => spawnSync('git', args, { cwd: root, encoding: 'utf8', env: { ...process.env, ...GIT_ENV } });
  if (git(['init', '-q', '-b', 'main']).status !== 0) return 'git init failed';
  for (const [index, files] of COMMITS.entries()) {
    for (const [name, text] of Object.entries(files)) fs.writeFileSync(path.join(root, name), text);
    for (const args of [['add', '-A'], ['-c', 'commit.gpgsign=false', 'commit', '-q', '-m', `change ${index}`]]) {
      const run = git(args);
      if (run.status !== 0) return `git ${args[0]} failed: ${run.stderr.trim()}`;
    }
  }
  return null;
}

const data = result => result.json?.results?.[0]?.data;

export async function checkGraphCochange(ctx, prepared, checks) {
  if (prepared) {
    checks.push(['graph.cochange.fixture', false, prepared]);
    return;
  }
  const query = { queries: [{ path: SCOPE }] };
  const built = data(await ctx.cliCall('graphBuild', query));
  const ask = { queries: [{ operation: 'cochange', path: SCOPE, graph: built.id, target: 'a.ts' }] };
  const outputs = {};
  for (const surface of ctx.surfaces) {
    const result = await ctx.callOn(surface)('graphQuery', ask);
    const answer = data(result);
    const rows = Object.fromEntries((answer?.rows ?? []).map(row => [path.basename(row.path ?? row.id ?? ''), row]));
    const expected = !result.isError
      && answer?.summary?.targetCommits === 4
      && rows['b.ts']?.commits === 2 && rows['b.ts']?.share === 0.5 && rows['b.ts']?.linked === true
      && rows['c.ts']?.commits === 1 && rows['c.ts']?.share === 0.25 && rows['c.ts']?.linked === false
      && (answer?.rows ?? [])[0] === rows['b.ts'];
    checks.push([`graph.operation.cochange.${surface}`, expected, result.text]);
    outputs[surface] = parityView(result.json, 'graphQuery');
  }
  const difference = firstDiff(outputs.cli, outputs.mcp);
  checks.push(['graph.operation.cochange.parity', !difference, difference ?? '']);
}
