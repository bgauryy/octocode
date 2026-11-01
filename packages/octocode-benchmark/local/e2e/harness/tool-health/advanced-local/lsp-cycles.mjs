import { equal, files, fixturePath, relative, scenario } from './helpers.mjs';

const workspaceRoot = fixturePath('code');
const path = `${workspaceRoot}/cycle.ts`;

const expected = {
  callees: [
    ['beta', 6, 1, 'alpha', [3]],
    ['gamma', 10, 1, 'alpha', [3]],
    ['alpha', 1, 2, 'beta', [7]],
    ['beta', 6, 2, 'gamma', [11]],
  ],
  callers: [
    ['beta', 6, 1, 'alpha', [7]],
    ['alpha', 1, 2, 'beta', [3]],
    ['gamma', 10, 2, 'beta', [11]],
    ['alpha', 1, 3, 'gamma', [3]],
  ],
};

export async function cyclicCalls(ctx, surface, checks) {
  for (const operation of ['callers', 'callees']) {
    for (const depth of [1, 20]) {
      const label = `lsp.cycle.${operation}.${depth}`;
      const result = await scenario(ctx, 'lspSearch', surface, { label, query: { workspaceRoot, path, operation, depth, symbolName: 'alpha', line: 1 }, checks, responseChars: 600 });
      const found = files(result.data).flatMap(file => file.matches.map(match => [
        relative(ctx, file.path), match.symbolName, match.line,
        match.via?.symbolName ?? 'alpha', match.sites.map(site => site.line),
      ]));
      equal(checks, `${label}.${surface}.edges`, found, expected[operation].filter(edge => edge[2] <= depth).map(([symbol, line, , via, sites]) => ['code/cycle.ts', symbol, line, via, sites]));
    }
  }
}
