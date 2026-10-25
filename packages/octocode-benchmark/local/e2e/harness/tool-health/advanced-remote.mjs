// Advanced remote behavior: assertions use explicit fixture facts, then drive
// the authored envelope and every required cursor through both built surfaces.
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { inventoryRows, nextHints, sourceLines, sourceView, structureFiles, rawPatch } from '../responses.mjs';
import { tokenFor, withoutReadinessPings } from './surfaces.mjs';
import { advancedRegistries } from './advanced-registry.mjs';
import { advancedHistoryForms } from './advanced-history.mjs';
import { addedLines, blobSha } from './mock-model.mjs';

const REPO = { owner: 'octo', repo: 'demo' };
const data = result => result?.json?.results?.[0]?.data ?? {};
const equal = (actual, expected) => JSON.stringify(actual) === JSON.stringify(expected);
const names = rows => (rows ?? []).map(row => row.repo);
const paths = rows => (rows ?? []).map(row => row.path).toSorted();
const numbers = rows => (rows ?? []).map(row => row.number);
const readerRequests = (ctx, surface) => withoutReadinessPings(ctx.mock.requestsFrom.filter(entry => entry.token === tokenFor('ghGetFileContent', surface)));
const sourceEvidence = result => !result?.isError && result?.json?.results?.[0]?.status === undefined
  && !data(result).errorCode && sourceLines(data(result)).length > 0;
const checks = async (ctx, cases) => {
  const out = [];
  for (const surface of ctx.surfaces) for (const [label, query, verify] of cases) {
    const result = await ctx.call(ctx.q(query), surface);
    out.push([`advanced.${label}.${surface}`, verify(data(result), result), JSON.stringify(result.json).slice(0, 700)]);
  }
  return out;
};
const error = errorCode => value => value.errorCode === errorCode;

async function repository(ctx) {
  const out = await checks(ctx, [
    ['search.rangeSort', { operation: 'search', keywords: ['demo'], filters: { stars: { min: 5, max: 50 }, language: 'TypeScript' }, sort: { by: 'stars', order: 'asc' } }, d => equal(names(d.repositories), ['demo-cli', 'demo'])],
    ['search.archiveOnly', { operation: 'search', owner: 'octo', filters: { archive: 'only' } }, d => equal(names(d.repositories), ['retired-greeter'])],
    ['list.defaultArchive', { operation: 'list', owner: 'octo' }, d => equal(names(d.repositories).toSorted(), ['demo', 'demo-cli', 'demo-docs'])],
    ['list.archiveOnly', { operation: 'list', owner: 'octo', filters: { archive: 'only' } }, d => equal(names(d.repositories), ['retired-greeter'])],
    ['get.fields', { ...REPO, operation: 'get', fields: ['stars'] }, d => d.repositories?.[0]?.stars === 42 && !Object.hasOwn(d.repositories[0], 'description') && d.repositories[0].repo === 'demo'],
    ['get.identity', { ...REPO, operation: 'get' }, d => d.repositories?.[0]?.owner === 'octo' && d.repositories?.[0]?.repo === 'demo'],
    ['get.missing', { ...REPO, operation: 'get', repo: 'missing-repository' }, error('notFound')],
  ]);
  for (const surface of ctx.surfaces) {
    const token = tokenFor('ghSearchRepo', surface);
    ctx.mock.model.githubResponders.set(token, r => r.p === '/search/repositories' ? { body: { total_count: 0, incomplete_results: true, items: [] } } : null);
    try {
      const result = await ctx.call(ctx.q({ operation: 'search', keywords: ['partial-proof'] }), surface);
      const d = data(result);
      out.push([`advanced.search.providerPartial.${surface}`, d.isPartial === true && d.partialReasons?.includes('providerIncompleteResults'), JSON.stringify(d)]);
    } finally { ctx.mock.model.githubResponders.delete(token); }
  }
  return out;
}

async function code(ctx) {
  return checks(ctx, [
    ['filters.intersection', { ...REPO, keywords: ['greet'], pathPrefix: 'src', nameContains: 'greeter', extensions: ['ts', 'md'] }, d => equal(paths(d.files), ['src/greeter.ts'])],
    ['path.only', { ...REPO, keywords: ['greeter'], matchIn: ['path'] }, d => equal(paths(d.files), ['src/greeter.ts']) && d.files.every(file => !file.lines?.length)],
    ['ref.reRead', { ...REPO, keywords: ['healthMarker'], pathPrefix: 'src', ref: ctx.mock.model.head.oid }, d => equal(paths(d.files), ['src/app.ts', 'src/greeter.ts', 'src/util.ts'])
      && d.files.every(file => file.owner === REPO.owner && file.repo === REPO.repo && file.commitSha === ctx.mock.model.head.oid)],
    ['scope.owner', { owner: 'octo', keywords: ['greeter'], extensions: ['md'] }, d => (d.files ?? []).some(file => file.path === 'README.md')],
  ]);
}

async function structure(ctx) {
  const out = await checks(ctx, [
    ['languages.bytes', { ...REPO, operation: 'languages' }, d => equal(d.languages, { TypeScript: 1200, Markdown: 300 })],
    ['refs.exact', { ...REPO, operation: 'refs', ref: ctx.mock.model.commits[0].oid }, d => d.commitSha === ctx.mock.model.commits[0].oid && d.refKind === 'commit'],
    ['tree.exactGlob', { ...REPO, operation: 'tree', include: ['src/greeter.ts'] }, d => equal(paths(structureFiles(d.files)), ['src/greeter.ts'])],
    ['tree.bareNameNoWidening', { ...REPO, operation: 'tree', include: ['greeter'] }, (d, r) => r.json?.results?.[0]?.status === 'empty' && !structureFiles(d.files).length],
    ['tree.branch', { ...REPO, operation: 'tree', ref: 'feature-4', include: ['pkg/mod00.ts', 'pkg/mod01.ts'] }, d => equal(paths(structureFiles(d.files)), ['pkg/mod00.ts', 'pkg/mod01.ts'])],
    ['tree.missingRef', { ...REPO, operation: 'tree', ref: 'missing-branch' }, error('notFound')],
  ]);
  for (const surface of ctx.surfaces) {
    const result = await ctx.call(ctx.q({ ...REPO, operation: 'tree', include: ['src/greeter.ts'], materialize: true }), surface);
    const location = data(result).location;
    const target = location?.localPath && path.join(location.localPath, 'src/greeter.ts');
    out.push([`advanced.materialize.content.${surface}`, !!target && fs.existsSync(target) && fs.readFileSync(target, 'utf8').includes('return { text: `hello ${name}` };'), JSON.stringify(data(result))]);
  }
  return out;
}

async function reader(ctx) {
  const out = await checks(ctx, [
    ['ranges.exact', { ...REPO, path: 'src/greeter.ts', ranges: [{ startLine: 2, endLine: 3 }, { startLine: 8, endLine: 8 }] }, d => equal(sourceLines(d), [[2, 'export interface Greeting {'], [3, '  text: string;'], [8, '  return { text: `hello ${name}` };']])],
    ['matcher.orContext', { ...REPO, path: 'src/greeter.ts', matchString: ['greeter-1', 'greeter-2'], contextLines: { before: 0, after: 1 } }, d => equal(sourceLines(d).map(([line]) => line), [7, 8, 15, 16])],
    ['matcher.wholeWordCase', { ...REPO, path: 'src/greeter.ts', matchString: ['Greeter'], caseMode: 'sensitive', wholeWord: true }, d => equal(sourceLines(d).map(([line]) => line), [11])],
    ['matcher.pcre2', { ...REPO, path: 'src/greeter.ts', matchString: ['(?<=healthMarker )greeter-\\d'], regexEngine: 'pcre2' }, d => equal(sourceLines(d).map(([line]) => line), [7, 15])],
    ['matcher.regex', { ...REPO, path: 'src/greeter.ts', matchString: ['^export (function|class)'], regexEngine: 'rust' }, d => equal(sourceLines(d).map(([line]) => line), [6, 11])],
    ['view.compact', { ...REPO, path: 'src/greeter.ts', view: 'compact' }, d => d.view === 'compact' && d.content.includes('function greet') && !d.content.includes('healthMarker')],
    ['view.symbols', { ...REPO, path: 'src/greeter.ts', view: 'symbols' }, d => d.view === 'symbols' && d.content.includes('Greeting') && d.content.includes('say(') && !d.content.includes('return this.prefix')],
    ['ref.branch', { ...REPO, path: 'pkg/mod00.ts', ref: 'feature-4' }, d => equal(sourceLines(d), [[1, '// healthMarker bulk-0'], [2, 'export const mod0 = 0;']])],
    ['path.missing', { ...REPO, path: 'src/missing.ts' }, error('notFound')],
  ]);
  for (const surface of ctx.surfaces) {
    const report = await ctx.explore('ghGetFileContent', { queries: [{ owner: 'octo', repo: 'demo-docs', path: 'guide.txt', forceRefresh: true }], responseChars: 4000 }, surface);
    const records = report.responses.filter(row => row.tool === 'ghGetFileContent').flatMap(row => row.result.json?.results?.flatMap(item => sourceLines(item.data)) ?? []);
    const expected = Array.from({ length: 1500 }, (_, i) => [i + 1, `Line ${i + 1}: the greeter guide repeats this sentence to fill a long document.`]);
    out.push([`advanced.cursor.exactEvidence.${surface}`, !report.failures.length && equal(records, expected), `${records.length} lines, ${report.responses.length} calls; ${JSON.stringify(report.failures)}`]);
    const first = report.responses[0]?.result;
    const lead = first && nextHints(first.json).find(call => call.kind === 'continue');
    if (lead) {
      const before = readerRequests(ctx, surface).filter(entry => entry.request.includes('/contents/guide.txt')).length;
      const a = await ctx.callOn(surface)(lead.tool, lead.input);
      const b = await ctx.callOn(surface)(lead.tool, lead.input);
      const after = readerRequests(ctx, surface).filter(entry => entry.request.includes('/contents/guide.txt')).length;
      const original = report.responses.find(row => row.tool === lead.tool && equal(row.input, lead.input))?.result;
      const exact = sourceEvidence(original) && sourceEvidence(a) && sourceEvidence(b)
        && equal(sourceLines(data(a)), sourceLines(data(original))) && equal(sourceLines(data(b)), sourceLines(data(original)));
      out.push([`advanced.cursor.immutableReplay.${surface}`, exact && before === after, `original page ${sourceLines(data(original)).length} lines; statuses ${[original,a,b].map(result => result?.json?.results?.[0]?.status).join(",")}; provider content calls ${before}→${after}`]);
    } else out.push([`advanced.cursor.issued.${surface}`, false, 'large source did not issue a continuation']);
  }
  return [...out, ...await readerForms(ctx), ...await readerFlows(ctx)];
}

const readRows = report => report.responses.filter(row => row.tool === 'ghGetFileContent').flatMap(row => row.result.json?.results ?? []);
const numberedSource = text => addedLines(text).map((line, index) => [index + 1, line]);
const readText = rows => rows.map(row => sourceView(row.data).text).join('');
const readEvidence = result => result.json.results.map(row => [row.index, row.status ?? null, row.data.errorCode ?? null, sourceView(row.data).text]);

async function readerForms(ctx) {
  const source = numberedSource(ctx.mock.model.files['src/greeter.ts']);
  const lines = (...wanted) => d => equal(sourceLines(d), source.filter(([line]) => wanted.includes(line)));
  const row = { ...REPO, path: 'src/greeter.ts' };
  const out = await checks(ctx, [
    ['ranges.union', { ...row, ranges: [{ startLine: 8, endLine: 9 }, { startLine: 6, endLine: 8 }, { startLine: 7, endLine: 7 }] }, lines(6, 7, 8, 9)],
    ['matcher.rangeIntersection', { ...row, ranges: [{ startLine: 6, endLine: 8 }], matchString: ['greeter-1', 'greeter-2', 'greeter-1'], contextLines: { before: 100, after: 100 } }, lines(6, 7, 8)],
    ['matcher.multilineContained', { ...row, ranges: [{ startLine: 6, endLine: 7 }], matchString: ['export function greet(name: string): Greeting {\n  // healthMarker greeter-1'] }, lines(6, 7)],
    ['matcher.multilineExcluded', { ...row, ranges: [{ startLine: 6, endLine: 6 }], matchString: ['export function greet(name: string): Greeting {\n  // healthMarker greeter-1'] }, (d, r) => r.json.results[0].status === 'empty' && d.content === '' && d.matchNotFound === true],
    ['ranges.block', { ...row, ranges: [{ startLine: 7, endLine: 7 }], block: true }, lines(6, 7, 8, 9)],
    ['ranges.clamped', { ...row, ranges: [{ startLine: 17, endLine: 100 }] }, d => lines(17, 18)(d) && d.warnings?.length > 0],
    ['matcher.blockOutsideRange', { ...row, ranges: [{ startLine: 7, endLine: 7 }], matchString: ['greeter-1'], block: true }, lines(6, 7, 8, 9)],
    ['matcher.smartPerTerm', { ...row, matchString: ['GREETER', 'greeting'], wholeWord: true }, lines(2, 6)],
    ['matcher.insensitive', { ...row, matchString: ['HEALTHMARKER'], caseMode: 'insensitive' }, lines(7, 15)],
    ['view.compactRange', { ...row, view: 'compact', ranges: [{ startLine: 6, endLine: 9 }] }, d => d.view === 'compact' && d.content.includes('`hello ${name}`') && d.content.includes('return') && !d.content.includes('healthMarker') && !d.content.includes('class Greeter')],
    ['view.compactBlock', { ...row, view: 'compact', ranges: [{ startLine: 7, endLine: 7 }], block: true }, d => d.view === 'compact' && d.content.includes('function greet') && d.content.includes('`hello ${name}`') && !d.content.includes('healthMarker')],
  ]);
  for (const surface of ctx.surfaces) for (const extra of [{ minify: true }, { matchString: 'greet' }, { view: 'compact', matchString: ['greet'] }, { view: 'symbols', ranges: [{ startLine: 1, endLine: 2 }] }]) {
    const before = readerRequests(ctx, surface).length;
    const result = await ctx.call(ctx.q({ ...row, ...extra }), surface);
    const failure = data(result).errorCode ?? result.json?.errorCode;
    out.push([`advanced.forms.rejected.${JSON.stringify(extra)}.${surface}`, failure === 'invalidInput' && readerRequests(ctx, surface).length === before, JSON.stringify(result.json)]);
  }
  return out;
}

function readFixtureReply(request, sources) {
  const target = request.p.replace('/repos/octo/demo/contents/', '');
  if (!Object.hasOwn(sources, target)) return null;
  const text = sources[target];
  if (String(request.headers.accept).includes('raw')) return { body: text, headers: { 'content-type': 'application/vnd.github.raw; charset=utf-8' } };
  return { body: { type: 'file', path: target, encoding: 'base64', size: Buffer.byteLength(text), content: Buffer.from(text).toString('base64'), sha: blobSha(text) } };
}

async function exactRead(ctx, surface, out, { label, query, expected, responseChars }) {
  const report = await ctx.explore('ghGetFileContent', { queries: [query], responseChars }, surface);
  const rows = readRows(report);
  const opaque = report.responses.flatMap(entry => nextHints(entry.result.json)).filter(call => call.kind === 'continue').every(call => equal(Object.keys(call.input), ['cursor']));
  out.push([`advanced.flow.${label}.${surface}`, opaque && !report.failures.length && rows.length > 0 && rows.every(row => !row.status && !row.data.errorCode) && readText(rows) === expected, `${rows.length} rows/${report.responses.length} calls; ${JSON.stringify(report.failures)}; expected ${Buffer.byteLength(expected)} bytes, got ${Buffer.byteLength(readText(rows))}`]);
  return report;
}

async function readerBatch(ctx, surface, out, sources) {
  const ref = ctx.mock.model.head.oid;
  const at = file => ({ ...REPO, ref, path: file });
  const report = await ctx.explore('ghGetFileContent', { responseChars: 500, queries: [
    { ...at('flow-lines.txt'), ranges: [{ startLine: 3, endLine: 2 }] },
    at('src/no-such-reader.ts'), at('flow-lines.txt'),
    { ...at('flow-lines.txt'), matchString: ['absent-needle'] },
    { ...at('src/greeter.ts'), ranges: [{ startLine: 2, endLine: 4 }] },
  ] }, surface);
  const rows = readRows(report);
  const forIndex = index => rows.filter(row => row.index === index);
  const failures = forIndex(0).length === 1 && forIndex(0)[0].data.errorCode === 'invalidInput'
    && forIndex(1).length === 1 && forIndex(1)[0].data.errorCode === 'notFound';
  const empty = forIndex(3).length === 1 && forIndex(3)[0].status === 'empty';
  const evidence = readText(forIndex(2)) === sources['flow-lines.txt']
    && equal(forIndex(4).flatMap(row => sourceLines(row.data)), [[2, 'export interface Greeting {'], [3, '  text: string;'], [4, '}']]);
  out.push([`advanced.flow.batchIndexes.${surface}`, !report.failures.length && failures && empty && evidence && rows.every(row => row.index >= 0 && row.index <= 4), `${report.responses.length} calls; indexes ${rows.map(row => row.index)}; ${JSON.stringify(report.failures)}`]);
  await readerBatchReplay(ctx, surface, out, report);
}

async function readerBatchReplay(ctx, surface, out, report) {
  const issued = report.responses.find(entry => entry.input.cursor && entry.result.json?.results?.some(row => row.index === 2 && row.data.content));
  if (!issued) { out.push([`advanced.flow.batchReplay.${surface}`, false, 'No issued page for batch index2']); return; }
  const before = readerRequests(ctx, surface).length;
  const replay = await ctx.callOn(surface)(issued.tool, issued.input);
  out.push([`advanced.flow.batchReplay.${surface}`, !replay.isError && equal(readEvidence(replay), readEvidence(issued.result)) && readerRequests(ctx, surface).length === before, JSON.stringify(readEvidence(replay))]);
}

async function readerMatchPages(ctx, surface, out, sources) {
  const query = { ...REPO, ref: ctx.mock.model.head.oid, path: 'flow-lines.txt', ranges: [{ startLine: 4, endLine: 22 }], matchString: ['needle', 'needle'], contextLines: { before: 0, after: 1 } };
  const report = await ctx.explore('ghGetFileContent', { queries: [query], responseChars: 1 }, surface);
  const expected = numberedSource(sources['flow-lines.txt']).filter(([line]) => line >= 4 && line <= 22 && line % 3 !== 0);
  const rows = readRows(report);
  const matches = rows.flatMap(row => row.data.matchedLines ?? []);
  out.push([`advanced.flow.matchPages.${surface}`, !report.failures.length && equal(rows.flatMap(row => sourceLines(row.data)), expected) && equal(matches, [4, 7, 10, 13, 16, 19, 22]), `${report.responses.length} calls; matched ${matches}`]);
  await exactRead(ctx, surface, out, {
    label: 'oversizedMatchRange', query: { ...query, path: 'flow-long.txt', matchString: ['0:界😀;', '5999:界😀;'], ranges: [{ startLine: 1, endLine: 1 }], contextLines: { before: 100, after: 100 } },
    expected: sources['flow-long.txt'].split('\n')[0] + '\n', responseChars: 1000,
  });
}

async function readerMatchAttribution(ctx, surface, out) {
  const query = { ...REPO, ref: ctx.mock.model.head.oid, path: 'flow-attribution.txt', matchString: ['(?<=prefix\n)same'], regexEngine: 'pcre2', contextLines: { before: 0, after: 1 } };
  const report = await ctx.explore('ghGetFileContent', { queries: [query], responseChars: 1 }, surface);
  const rows = readRows(report);
  const attribution = rows.map(row => ({ lines: sourceLines(row.data), matched: row.data.matchedLines }));
  out.push([`advanced.flow.identicalContextAttribution.${surface}`, !report.failures.length && equal(attribution, [
    { lines: [[2, 'same']], matched: [2] }, { lines: [[3, 'same']], matched: [] },
  ]), JSON.stringify(attribution)]);
}

async function readerRecoveries(ctx, surface, out) {
  const source = numberedSource(ctx.mock.model.files['src/greeter.ts']);
  for (const scenario of [
    { label: 'compactComments', fields: { view: 'compact', ranges: [{ startLine: 7, endLine: 7 }] }, status: 'empty', lines: [7] },
    { label: 'pastEnd', fields: { ranges: [{ startLine: 99, endLine: 100 }] }, status: 'error', lines: [17, 18] },
  ]) {
    const initial = await ctx.call(ctx.q({ ...REPO, path: 'src/greeter.ts', ...scenario.fields }), surface);
    const repair = nextHints(initial.json).find(call => call.kind === 'related' && call.tool === 'ghGetFileContent');
    const report = repair && await ctx.explore(repair.tool, repair.input, surface);
    const expected = source.filter(([line]) => scenario.lines.includes(line));
    out.push([`advanced.flow.${scenario.label}Recovery.${surface}`, initial.json.results[0].status === scenario.status && !!report && !report.failures.length && equal(readRows(report).flatMap(row => sourceLines(row.data)), expected), JSON.stringify({ initial: initial.json, repair })]);
  }
}

async function readerSymbols(ctx, surface, out) {
  const report = await ctx.explore('ghGetFileContent', { queries: [{ ...REPO, path: 'src/greeter.ts', view: 'symbols' }], responseChars: 1 }, surface);
  const source = addedLines(ctx.mock.model.files['src/greeter.ts']);
  const expected = [[2, 4], [3, 3], [6, 9], [11, 18], [12, 12], [14, 17]].map(([start, end]) => `${start === end ? start : `${start}-${end}`}\t${source[start - 1]}`);
  const rows = readRows(report);
  const actual = rows.flatMap(row => (row.data.content ?? '').split('\n').filter(Boolean));
  out.push([`advanced.flow.symbolPages.${surface}`, !report.failures.length && rows.every(row => !row.status && row.data.view === 'symbols') && equal(actual, expected), `${report.responses.length} calls; ${JSON.stringify(actual)}`]);
}

async function readerPinned(ctx, surface, out, sources) {
  const token = tokenFor('ghGetFileContent', surface);
  let moved = false;
  const commits = [ctx.mock.model.head.oid, ctx.mock.model.commits[0].oid];
  const original = sources['flow-lines.txt'];
  const changed = original.replaceAll('source', 'changed');
  ctx.mock.model.githubResponders.set(token, request => {
    if (request.p === '/repos/octo/demo/commits/reader-moving') return { body: commits[Number(moved)] };
    return readFixtureReply(request, { 'flow-pinned.txt': request.url.searchParams.get('ref') === commits[0] ? original : changed });
  });
  const query = { ...REPO, path: 'flow-pinned.txt', ref: 'reader-moving', forceRefresh: true };
  const first = await ctx.call({ queries: [query], responseChars: 500 }, surface);
  const continuation = nextHints(first.json).find(call => call.kind === 'continue');
  moved = true;
  const before = readerRequests(ctx, surface).length;
  const report = continuation && await ctx.explore(continuation.tool, continuation.input, surface);
  const originalRows = [first.json.results[0], ...(report ? readRows(report) : [])];
  out.push([`advanced.flow.refPinned.${surface}`, !!report && !report.failures.length && data(first).commitSha === commits[0] && readText(originalRows) === original && readerRequests(ctx, surface).length === before, `${originalRows.length} rows; provider calls ${before}→${readerRequests(ctx, surface).length}`]);
  const fresh = await exactRead(ctx, surface, out, { label: 'forceRefreshMoves', query, expected: changed, responseChars: 500 });
  out.push([`advanced.flow.newCommit.${surface}`, data(fresh.responses[0]?.result).commitSha === commits[1], JSON.stringify(data(fresh.responses[0]?.result))]);
}

async function readerFlows(ctx) {
  const out = [];
  const sources = {
    'flow-lines.txt': Array.from({ length: 24 }, (_, index) => `source ${index + 1}: 界😀 café\t${index % 3 === 0 ? 'needle' : 'plain'}`).join('\r\n') + '\r\n',
    'flow-long.txt': Array.from({ length: 6000 }, (_, index) => `${index}:界😀;`).join('') + '\r\ntail界😀',
    'flow-data.json': JSON.stringify({ marker: 'literal // text', unicode: '界😀', values: [0, false, null, { nested: 'café' }] }, null, 2) + '\n',
    'flow-empty.txt': '',
    'flow-attribution.txt': 'prefix\nsame\nsame\n',
  };
  for (const surface of ctx.surfaces) {
    const token = tokenFor('ghGetFileContent', surface);
    ctx.mock.model.githubResponders.set(token, request => readFixtureReply(request, sources));
    try {
      const at = file => ({ ...REPO, path: file, ref: ctx.mock.model.head.oid });
      await exactRead(ctx, surface, out, { label: 'unicodeCrlfTiny', query: at('flow-lines.txt'), expected: sources['flow-lines.txt'], responseChars: 1 });
      await exactRead(ctx, surface, out, { label: 'oversizedUnicode', query: at('flow-long.txt'), expected: sources['flow-long.txt'], responseChars: 1000 });
      await exactRead(ctx, surface, out, { label: 'zeroBytes', query: at('flow-empty.txt'), expected: '', responseChars: 1 });
      const compact = await ctx.explore('ghGetFileContent', { queries: [{ ...at('flow-data.json'), view: 'compact' }], responseChars: 1 }, surface);
      const compactRows = readRows(compact);
      let compactValue;
      try { compactValue = JSON.parse(readText(compactRows)); } catch { compactValue = null; }
      out.push([`advanced.flow.compactJson.${surface}`, !compact.failures.length && compactRows.every(row => row.data.view === 'compact') && equal(compactValue, JSON.parse(sources['flow-data.json'])), readText(compactRows)]);
      await readerBatch(ctx, surface, out, sources);
      await readerMatchPages(ctx, surface, out, sources);
      await readerMatchAttribution(ctx, surface, out);
      await readerSymbols(ctx, surface, out);
      await readerRecoveries(ctx, surface, out);
      await readerPinned(ctx, surface, out, sources);
    } finally { ctx.mock.model.githubResponders.delete(token); }
  }
  return out;
}

async function history(ctx) {
  const out = await checks(ctx, [
    ['commits.pathDate', { ...REPO, operation: 'commit', pathPrefix: 'src', filters: { committedAt: { from: '2025-01-03T00:00:00Z', to: '2025-01-03T23:59:59Z' } } }, d => equal((d.commits ?? []).map(c => c.commitSha), [ctx.mock.model.head.oid])],
    ['commits.message', { ...REPO, operation: 'commit', keywords: ['src/greeter.ts'] }, d => equal((d.commits ?? []).map(c => c.commitSha), [ctx.mock.model.head.oid])],
    ['commits.ref', { ...REPO, operation: 'commit', ref: ctx.mock.model.commits[0].oid }, d => equal((d.commits ?? []).map(c => c.commitSha), [ctx.mock.model.commits[0].oid])],
    ['prs.open', { ...REPO, operation: 'pullRequest', filters: { state: 'open' } }, d => equal(numbers(d.pullRequests).toSorted((a, b) => a - b), [3, 4])],
    ['prs.merged', { ...REPO, operation: 'pullRequest', filters: { isMerged: true } }, d => equal(numbers(d.pullRequests), [1])],
    ['prs.branchSelection', { ...REPO, operation: 'pullRequest', filters: { headRef: { include: ['feature-4'], exclude: ['feature-3'] }, baseRef: { include: ['main'] } } }, d => equal(numbers(d.pullRequests), [4])],
    ['issues.author', { ...REPO, operation: 'issue', keywords: ['prefix'], filters: { author: { include: ['octo-dev'] } } }, d => equal(numbers(d.issues), [2])],
    ['issues.excludedAuthor', { ...REPO, operation: 'issue', keywords: ['prefix'], filters: { author: { exclude: ['octo-dev'] } } }, (d, r) => r.json?.results?.[0]?.status === 'empty' && !d.issues?.length],
  ]);
  for (const surface of ctx.surfaces) {
    const report = await ctx.explore('ghSearchHistory', { queries: [{ ...REPO, operation: 'issue', keywords: ['Paged history'] }], responseChars: 3000 }, surface);
    const actual = report.responses.filter(row => row.tool === 'ghSearchHistory').flatMap(row => row.result.json?.results?.flatMap(item => numbers(item.data?.issues)) ?? []);
    out.push([`advanced.issue.nativeAndResponsePaging.${surface}`, !report.failures.length && equal(actual, Array.from({ length: 51 }, (_, i) => 150 - i)), `${actual.length} issues, ${report.responses.length} calls; ${JSON.stringify(report.failures)}`]);
  }
  return out;
}

const files = d => inventoryRows(d.files ?? d.commits?.[0]?.files ?? d.pullRequests?.[0]?.files);

async function historyItem(ctx) {
  const out = await checks(ctx, [
    ['issue.discussion', { ...REPO, operation: 'issue', number: 2, sections: ['body', 'comments'] }, d => d.issues?.[0]?.body === 'The greeter needs a configurable prefix.' && d.issues?.[0]?.comments?.[0]?.body === 'Agreed, the Greeter class takes one now.'],
    ['commit.fileFilter', { ...REPO, operation: 'commit', ref: 'main', sections: ['files'], include: ['src/*.ts'], fileStatus: ['added'], minChanges: 10 }, d => equal(paths(files(d)), ['src/greeter.ts', 'src/util.ts'])],
    ['compare.files', { ...REPO, operation: 'compare', baseRef: ctx.mock.model.commits[0].oid, headRef: 'main', sections: ['files'], include: ['src/*.ts'] }, d => d.headSha === ctx.mock.model.head.oid && equal(paths(files(d)), ['src/app.ts', 'src/greeter.ts', 'src/util.ts'])],
    ['pr.sectionCommits', { ...REPO, operation: 'pullRequest', number: 4, sections: ['commits'] }, d => d.pullRequests?.[0]?.commits?.[0]?.commitSha === ctx.mock.model.branchCommits[0].oid],
    ['pr.patchMatch', { ...REPO, operation: 'pullRequest', number: 4, sections: ['patches'], include: ['pkg/huge.ts'], matchString: ['huge line 2999'], contextLines: { before: 1, after: 1 } }, d => { const patch = rawPatch(files(d)[0]?.patch); return patch.includes('+export const huge2998 = 2998; // healthMarker huge line 2999') && !patch.includes('huge line 1\n'); }],
    ['issue.missing', { ...REPO, operation: 'issue', number: 99999 }, error('notFound')],
  ]);
  out.push(...await advancedHistoryForms(ctx));
  for (const surface of ctx.surfaces) {
    const report = await ctx.explore('ghGetHistoryItem', { queries: [{ ...REPO, operation: 'pullRequest', number: 4, sections: ['patches'], include: ['pkg/huge.ts'] }], responseChars: 6000 }, surface);
    const lines = report.responses.filter(row => row.tool === 'ghGetHistoryItem').flatMap(row => row.result.json?.results?.flatMap(item => files(item.data).flatMap(file => rawPatch(file.patch).split('\n').filter(line => line.startsWith('+export const')))) ?? []);
    const expected = Array.from({ length: 3000 }, (_, i) => `+export const huge${i} = ${i}; // healthMarker huge line ${i + 1}`);
    out.push([`advanced.patch.exactOnce.${surface}`, !report.failures.length && equal(lines, expected), `${lines.length} patch lines, ${report.responses.length} calls; ${JSON.stringify(report.failures)}`]);
    const commit = await ctx.explore('ghGetHistoryItem', { queries: [{ ...REPO, operation: 'commit', ref: 'main', sections: ['patches'], include: ['src/greeter.ts'] }], responseChars: 20000 }, surface, { maxPages: 12 });
    const commitCalls = nextHints(commit.responses[0]?.result.json);
    const summary = commitCalls.find(call => call.kind === 'related' && call.tool === 'ghGetHistoryItem' && call.input.queries?.some(row => row.operation === 'commit' && !row.sections?.includes('patches')));
    out.push([`advanced.commit.requestedEvidenceTerminates.${surface}`, !commit.failures.length && commit.responses.length === 1 && !!summary, `${commit.responses.length} required calls; ${JSON.stringify(commit.failures)}`]);
    for (const section of ['reviews', 'reviewComments']) {
      const read = await ctx.explore('ghGetHistoryItem', { queries: [{ ...REPO, operation: 'pullRequest', number: 4, sections: section === 'reviews' ? ['body', section] : [section] }], responseChars: 4000 }, surface);
      const evidence = read.responses.filter(row => row.tool === 'ghGetHistoryItem').flatMap(row => row.result.json?.results?.flatMap(item => (item.data?.pullRequests ?? []).flatMap(pr => (pr[section] ?? []).map(entry => [entry.id, entry.body]))) ?? []);
      const expectedReviews = Array.from({ length: 105 }, (_, i) => [String((section === 'reviews' ? 4000 : 5000) + i), `${section === 'reviews' ? 'Review' : 'Inline review'} evidence ${i}`]);
      out.push([`advanced.${section}.exactOnce.${surface}`, !read.failures.length && equal(evidence, expectedReviews), `${evidence.length} rows, ${read.responses.length} calls; ${JSON.stringify(read.failures)}`]);
    }
  }
  return out;
}

async function artifacts(ctx) {
  const registryUrl = ctx.mock.url;
  const out = await checks(ctx, [
    ['resolve.exact', { operation: 'resolve', ecosystem: 'npm', packageName: 'tiny-health', version: '1.0.0', registryUrl }, d => d.artifacts?.[0]?.version === '1.0.0' && equal(d.artifacts[0].dependencies, ['tiny-greet@^2.0.0'])],
    ['resolve.range', { operation: 'resolve', ecosystem: 'npm', packageName: 'tiny-health', version: '^1.0.0', registryUrl }, d => d.artifacts?.[0]?.version === '1.2.3'],
    ['resolve.tag', { operation: 'resolve', ecosystem: 'npm', packageName: 'tiny-greet', version: 'latest', registryUrl }, d => d.artifacts?.[0]?.version === '2.1.0' && d.artifacts[0].ref === ctx.mock.model.head.oid && !Object.hasOwn(d.artifacts[0], 'sourceRef')],
    ['resolve.missingVersion', { operation: 'resolve', ecosystem: 'npm', packageName: 'tiny-health', version: '9.0.0', registryUrl }, error('versionNotFound')],
    ['search.intersection', { operation: 'search', ecosystem: 'npm', keywords: ['health', 'greet'], registryUrl }, d => equal((d.artifacts ?? []).map(a => a.packageName), ['tiny-greet'])],
  ]);
  return [...out, ...await advancedRegistries(ctx)];
}

const count = cwd => Number(spawnSync('git', ['rev-list', '--count', 'HEAD'], { cwd, encoding: 'utf8' }).stdout?.trim());
const sparseFiles = (location, included, excluded) => !!location && fs.existsSync(path.join(location, included)) && !fs.existsSync(path.join(location, excluded));

async function clone(ctx) {
  const call = async query => data(await ctx.call(ctx.q({ ...REPO, ...query }), 'cli'));
  const first = await call({ paths: ['src'], historyDepth: 1 });
  const second = await call({ paths: ['docs'], historyDepth: 1 });
  const deep = await call({ paths: ['src'], historyDepth: 2 });
  const a = first.location?.localPath, b = second.location?.localPath, c = deep.location?.localPath;
  const out = [
    ['advanced.clone.sparseIdentity', a !== b && sparseFiles(a, 'src/greeter.ts', 'docs/notes.md') && sparseFiles(b, 'docs/notes.md', 'src/greeter.ts'), JSON.stringify({ first, second })],
    ['advanced.clone.depthIdentity', !!a && !!c && a !== c && count(a) === 1 && count(c) === 2, JSON.stringify(deep)],
  ];
  if (a) {
    const file = path.join(a, 'src/greeter.ts');
    fs.appendFileSync(file, '\n// local edit retained\n');
    const refreshed = await call({ paths: ['src'], historyDepth: 1, forceRefresh: true });
    out.push(['advanced.clone.preserveLocalEdits', refreshed.errorCode === 'checkoutDirty' && fs.readFileSync(file, 'utf8').endsWith('// local edit retained\n'), JSON.stringify(refreshed)]);
  }
  return out;
}

export const ADVANCED_REMOTE = {
  ghSearchRepo: repository, ghSearchCode: code, ghStructure: structure, ghGetFileContent: reader,
  ghSearchHistory: history, ghGetHistoryItem: historyItem, artifactSearch: artifacts, ghCloneRepo: clone,
};
