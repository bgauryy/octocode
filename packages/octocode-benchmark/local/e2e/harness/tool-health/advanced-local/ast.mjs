import fs from 'node:fs';
import path from 'node:path';
import { declarations } from '../../responses.mjs';
import { equal, orderedEqual, files, fixturePath, matches, relative, scenario, sourceText } from './helpers.mjs';

const model = fixturePath('code/model.ts');
const matchQuery = { path: model, operation: 'match', rule: { pattern: 'add($A, $B)' } };

function sourceByte(lines, line, column) {
  if (line < 1 || line > lines.length || column < 1 || column > lines[line - 1].length + 1) return undefined;
  return Buffer.byteLength(lines.slice(0, line - 1).join('\n')) + (line > 1 ? 1 : 0)
    + Buffer.byteLength(lines[line - 1].slice(0, column - 1));
}

function nodeRangeMatchesSource(row, lines) {
  const span = row.node?.match(/ (\d+):(\d+)-(\d+):(\d+)(?: \^\d+)?$/);
  return !!span && row.sourceRange?.startByte === sourceByte(lines, Number(span[1]), Number(span[2]))
    && row.sourceRange?.endByte === sourceByte(lines, Number(span[3]), Number(span[4]));
}

async function matching(ctx, surface, checks) {
  const content = await scenario(ctx, 'astSearch', surface, { label: 'ast.captures', query: { ...matchQuery, includeCaptures: true }, checks, responseChars: 200 });
  equal(checks, `ast.captured-values.${surface}`, matches(content.data).map(m => [m.line, m.value, m.metavarRanges?.A?.[0]?.text, m.metavarRanges?.B?.[0]?.text]), [[19, 'add(1, 2)', '1', '2'], [20, 'add(twice, 3)', 'twice', '3']]);
  for (const view of ['files', 'countMatches']) {
    const result = await scenario(ctx, 'astSearch', surface, { label: `ast.${view}`, query: { ...matchQuery, view }, checks, responseChars: 200 });
    equal(checks, `ast.${view}.${surface}`, files(result.data).map(file => view === 'files' ? relative(ctx, file.path) : [relative(ctx, file.path), file.matchCount]), view === 'files' ? ['code/model.ts'] : [['code/model.ts', 2]]);
  }
  const scoped = await scenario(ctx, 'astSearch', surface, { label: 'ast.scope', query: { ...matchQuery, scope: { start: { line: 19, column: 1 }, end: { line: 20, column: 1 }, mode: 'contained' } }, checks });
  equal(checks, `ast.exclusive-scope.${surface}`, matches(scoped.data).map(m => m.line), [19]);
  const rule = { all: [{ pattern: 'add($A, $B)' }, { inside: { kind: 'return_statement', stopBy: 'end' } }] };
  const relational = await scenario(ctx, 'astSearch', surface, { label: 'ast.relational-rule', query: { ...matchQuery, rule }, checks });
  equal(checks, `ast.structural-ancestry.${surface}`, matches(relational.data).map(m => m.line), [20]);
  const custom = await scenario(ctx, 'astSearch', surface, { label: 'ast.parser-glob', query: { path: fixturePath('code'), operation: 'match', rule: { pattern: 'add(8, 9)' }, parserGlobs: { typescript: ['*.source'] }, include: ['*.source'] }, checks });
  equal(checks, `ast.custom-extension.${surface}`, matches(custom.data).map(m => [relative(ctx, m.path), m.value]), [['code/custom.source', 'add(8, 9)']]);
}

async function symbolsAndTree(ctx, surface, checks) {
  const symbols = await scenario(ctx, 'astSearch', surface, { label: 'ast.symbols', query: { path: model, operation: 'symbols', symbolKinds: ['function'] }, checks, responseChars: 250 });
  const names = symbols.data.flatMap(data => declarations({ sc: { results: [{ data }] } })).map(d => [d.name, d.line, d.endLine ?? d.line]);
  equal(checks, `ast.function-boundaries.${surface}`, names, [['add', 14, 16], ['calculate', 18, 21], ['invoke', 25, 25]]);
  const exact = await scenario(ctx, 'astSearch', surface, { label: 'ast.symbol-filter', query: { path: model, operation: 'symbols', symbolFilter: { terms: ['Square'], mode: 'exactFirst' }, symbolKinds: ['class'] }, checks });
  equal(checks, `ast.exact-first.${surface}`, exact.data.flatMap(data => declarations({ sc: { results: [{ data }] } })).map(d => d.name), ['Square']);
  const substring = await scenario(ctx, 'astSearch', surface, { label: 'ast.substring', query: { path: model, operation: 'symbols', symbolFilter: { terms: ['square'], mode: 'substring', caseMode: 'insensitive' }, symbolKinds: ['class'] }, checks });
  equal(checks, `ast.substring.${surface}`, substring.data.flatMap(data => declarations({ sc: { results: [{ data }] } })).filter(d => d.kind === 'class').map(d => d.name), ['Square', 'LargeSquare']);
  for (const namedOnly of [true, false]) {
    const tree = await scenario(ctx, 'astSearch', surface, { label: `ast.syntax-tree.${namedOnly}`, query: { path: model, operation: 'syntaxTree', line: 14, namedOnly }, checks, responseChars: 500 });
    const nodes = tree.data.flatMap(data => data.nodes ?? []);
    checks.push([`ast.tree-function.${namedOnly}.${surface}`, nodes.some(row => row.node?.includes('function_declaration 14:')) && nodes.every(row => !row.node?.includes('class_declaration')), JSON.stringify(nodes)]);
    checks.push([`ast.tree-punctuation.${namedOnly}.${surface}`, nodes.some(row => / "[(){}]" /.test(row.node)) === !namedOnly, JSON.stringify(nodes)]);
    const lines = sourceText(ctx, 'code/model.ts').split('\n');
    checks.push([`ast.tree-source-ranges.${namedOnly}.${surface}`, nodes.length > 0 && nodes.every(row => nodeRangeMatchesSource(row, lines))
      && tree.data.every(data => !Object.hasOwn(data, 'byteRanges')), JSON.stringify(nodes)]);
  }
}

async function treeFragments(ctx, surface, checks) {
  const query = { path: model, operation: 'syntaxTree', namedOnly: false };
  const whole = await scenario(ctx, 'astSearch', surface, { label: 'ast.tree-whole', query, checks });
  const expected = whole.data.flatMap(data => data.nodes ?? []);
  const paged = await scenario(ctx, 'astSearch', surface, { label: 'ast.tree-fragments', query, checks, responseChars: 200, maxPages: expected.length + 1 });
  const nodes = paged.data.flatMap(data => data.nodes ?? []);
  orderedEqual(checks, `ast.tree-fragments.exact-evidence.${surface}`, nodes, expected);
  const lines = sourceText(ctx, 'code/model.ts').split('\n');
  checks.push([`ast.tree-fragments.atomic-coordinates.${surface}`, paged.pages > 1 && nodes.length > 0
    && nodes.every(row => nodeRangeMatchesSource(row, lines))
    && paged.data.every(data => !Object.hasOwn(data, 'byteRanges')), JSON.stringify(nodes)]);
}

async function treeSourceCoordinates(ctx, surface, checks) {
  const token = `ghp_${'a'.repeat(36)}`;
  const declaration = 'function after() { return 7; }';
  const source = `const marker = '😀é'; const value = "${token}"; ${declaration}\n`;
  const relativePath = fixturePath(`redaction-${surface}.ts`);
  const file = path.join(ctx.cwd, relativePath);
  fs.writeFileSync(file, source);
  try {
    const tree = await scenario(ctx, 'astSearch', surface, {
      label: 'ast.tree-source-coordinates', query: { path: relativePath, operation: 'syntaxTree', namedOnly: false }, checks, responseChars: 200,
    });
    const nodes = tree.data.flatMap(row => row.nodes ?? []);
    const functions = nodes.filter(row => row.node?.split(' ')[1].split(':').at(-1) === 'function_declaration');
    const startByte = Buffer.byteLength(source.slice(0, source.indexOf(declaration)));
    equal(checks, `ast.tree-source-coordinates.function.${surface}`, functions.map(row => row.sourceRange), [{ startByte, endByte: startByte + Buffer.byteLength(declaration) }]);
    checks.push([`ast.tree-source-coordinates.all-nodes.${surface}`, nodes.length > 0 && nodes.every(row => nodeRangeMatchesSource(row, source.split('\n'))), JSON.stringify(nodes)]);
    checks.push([`ast.tree-source-coordinates.no-source-token.${surface}`, !JSON.stringify(tree.responses).includes(token), 'Syntax rows expose grammar labels and coordinates only.']);
  } finally {
    fs.rmSync(file);
  }
}

export async function advancedAst(ctx) {
  const checks = [];
  for (const surface of ctx.surfaces) {
    await matching(ctx, surface, checks);
    await symbolsAndTree(ctx, surface, checks);
    await treeFragments(ctx, surface, checks);
    await treeSourceCoordinates(ctx, surface, checks);
  }
  return checks;
}
