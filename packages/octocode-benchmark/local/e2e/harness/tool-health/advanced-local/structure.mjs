import fs from 'node:fs';
import path from 'node:path';
import { structureFiles } from '../../responses.mjs';
import { equal, orderedEqual, fixturePath, relative, scenario } from './helpers.mjs';

const scope = fixturePath('structure');
const allFiles = ['structure/a.txt', 'structure/empty.txt', 'structure/large.txt', 'structure/sub/deep.txt', 'structure/sub/deeper/end.log'];
const project = (ctx, data) => data.flatMap(row => structureFiles(row.files)).map(file => ({ ...file, path: relative(ctx, file.path) }));

async function filters(ctx, surface, checks) {
  const base = { path: scope, operation: 'files', entryType: 'file' };
  const stat = fs.statSync(path.join(ctx.cwd, scope, 'large.txt'));
  const cases = [
    { label: 'files', query: base, expected: allFiles },
    { label: 'directories', query: { path: scope, operation: 'files', entryType: 'directory' }, expected: ['structure/sub', 'structure/sub/deeper'] },
    { label: 'depth', query: { ...base, minDepth: 2, maxDepth: 2 }, expected: ['structure/sub/deep.txt'] },
    { label: 'extension-name', query: { ...base, extensions: ['txt'], nameRegex: '^(a|large)\\.txt$' }, expected: ['structure/a.txt', 'structure/large.txt'] },
    { label: 'empty', query: { ...base, empty: true }, expected: ['structure/empty.txt'] },
    { label: 'size', query: { ...base, sizeBytes: { min: stat.size, max: stat.size } }, expected: ['structure/large.txt'] },
    { label: 'time', query: { ...base, include: ['large.txt'], modifiedAt: { from: new Date(stat.mtimeMs - 1000).toISOString(), to: new Date(stat.mtimeMs + 1000).toISOString() } }, expected: ['structure/large.txt'] },
    { label: 'time-excluded', query: { ...base, include: ['large.txt'], modifiedAt: { to: new Date(stat.mtimeMs - 1000).toISOString() } }, expected: [] },
    { label: 'hidden', query: { ...base, includeHidden: true, include: ['.*'] }, expected: ['structure/.hidden.txt'] },
    { label: 'permission', query: { ...base, include: ['large.txt'], permissions: (stat.mode & 0o777).toString(8), access: 'readable' }, expected: ['structure/large.txt'] },
    { label: 'permission-excluded', query: { ...base, include: ['large.txt'], permissions: ((stat.mode & 0o777) ^ 0o100).toString(8) }, expected: [] },
    { label: 'exclude', query: { ...base, include: ['**/*.txt'], exclude: ['sub/**'] }, expected: ['structure/a.txt', 'structure/empty.txt', 'structure/large.txt'] },
  ];
  for (const { label, query, expected } of cases) {
    const result = await scenario(ctx, 'structureSearch', surface, { label: `structure.${label}`, query: query, checks, responseChars: 250 });
    equal(checks, `structure.${label}.${surface}`, project(ctx, result.data).map(file => file.path), expected);
  }
}

async function treeAndOrder(ctx, surface, checks) {
  const tree = await scenario(ctx, 'structureSearch', surface, { label: 'structure.tree', query: { path: scope, operation: 'tree', maxDepth: 2 }, checks, responseChars: 250 });
  equal(checks, `structure.tree.${surface}`, project(ctx, tree.data).map(file => file.path), ['structure/a.txt', 'structure/empty.txt', 'structure/large.txt', 'structure/sub', 'structure/sub/deep.txt', 'structure/sub/deeper']);
  const sorted = await scenario(ctx, 'structureSearch', surface, { label: 'structure.sort', query: { path: scope, operation: 'files', entryType: 'file', include: ['*.txt'], maxDepth: 1, sort: { by: 'sizeBytes', order: 'desc' } }, checks });
  orderedEqual(checks, `structure.byte-order.${surface}`, project(ctx, sorted.data).map(file => [file.path, file.size]), [['structure/large.txt', 31], ['structure/a.txt', 4], ['structure/empty.txt', 0]]);
  const capped = await scenario(ctx, 'structureSearch', surface, { label: 'structure.capped', query: { path: scope, operation: 'files', entryType: 'file', limit: 2 }, checks });
  const first = capped.root.results[0].data;
  // A page that ends at `limit` is paged, not partial: hasMore plus its next page.
  checks.push([`structure.capped-disclosure.${surface}`, project(ctx, [first]).length <= 2 && first.pagination?.hasMore === true && Boolean(first.next?.nextPage) && first.isPartial !== true, JSON.stringify(first)]);
  equal(checks, `structure.resumed-cap.${surface}`, project(ctx, capped.data).map(file => file.path), allFiles);
}

export async function advancedStructure(ctx) {
  const checks = [];
  for (const surface of ctx.surfaces) {
    await filters(ctx, surface, checks);
    await treeAndOrder(ctx, surface, checks);
  }
  return checks;
}
