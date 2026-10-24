import fs from 'node:fs';
import path from 'node:path';
import { sourceLines, sourceView } from '../../responses.mjs';
import { equal, orderedEqual, fixturePath, scenario, sourcePairs, sourceText } from './helpers.mjs';

const windows = fixturePath('read/windows.txt');
const model = fixturePath('code/model.ts');
const pairs = data => data.flatMap(sourceLines);

async function selections(ctx, surface, checks) {
  const source = sourcePairs(sourceText(ctx, 'read/windows.txt'));
  const cases = [
    { label: 'whole-crlf', query: { path: windows }, expected: source },
    { label: 'disjoint-overlap', query: { path: windows, ranges: [{ startLine: 4, endLine: 5 }, { startLine: 1, endLine: 2 }, { startLine: 2, endLine: 4 }] }, expected: source.slice(0, 5) },
    { label: 'literal-context', query: { path: windows, matchString: ['alpha', 'last'], caseMode: 'sensitive', wholeWord: true, contextLines: { before: 1, after: 0 } }, expected: source.filter(([n]) => [3, 4, 6, 7].includes(n)) },
    { label: 'range-match', query: { path: windows, ranges: [{ startLine: 4, endLine: 6 }], matchString: ['alpha'], caseMode: 'insensitive', wholeWord: true }, expected: source.filter(([n]) => [4, 5].includes(n)) },
    { label: 'rust-regex', query: { path: windows, matchString: ['^fourth \\w+$'], regexEngine: 'rust' }, expected: source.filter(([n]) => n === 4) },
    { label: 'pcre2-regex', query: { path: windows, matchString: ['(?<=fourth )alpha'], regexEngine: 'pcre2' }, expected: source.filter(([n]) => n === 4) },
  ];
  for (const { label, query, expected } of cases) {
    const result = await scenario(ctx, 'localFetch', surface, { label: `read.${label}`, query: query, checks, responseChars: 500 });
    orderedEqual(checks, `read.${label}.${surface}`, pairs(result.data), expected);
  }
  const expectedBlock = sourcePairs(sourceText(ctx, 'code/model.ts')).filter(([n]) => n >= 18 && n <= 21);
  for (const selection of [{ ranges: [{ startLine: 19, endLine: 19 }] }, { matchString: ['add(twice, 3)'] }]) {
    const label = 'ranges' in selection ? 'range-block' : 'match-block';
    const result = await scenario(ctx, 'localFetch', surface, { label: `read.${label}`, query: { path: model, ...selection, block: true }, checks, responseChars: 400 });
    orderedEqual(checks, `read.${label}.${surface}`, pairs(result.data), expectedBlock);
  }
}

async function views(ctx, surface, checks) {
  const json = await scenario(ctx, 'localFetch', surface, { label: 'read.compact-json', query: { path: fixturePath('read/data.json'), view: 'compact' }, checks });
  const output = json.data.map(data => sourceView(data).text).join('');
  checks.push([`read.compact-json.${surface}`, JSON.stringify(JSON.parse(output)) === JSON.stringify(JSON.parse(sourceText(ctx, 'read/data.json'))) && json.data[0].view === 'compact', output]);
  const range = await scenario(ctx, 'localFetch', surface, { label: 'read.compact-range', query: { path: model, view: 'compact', ranges: [{ startLine: 14, endLine: 16 }] }, checks });
  checks.push([`read.compact-selection.${surface}`, range.data.every(data => data.view === 'compact') && range.data.map(data => sourceView(data).text).join('').includes('return a + b'), JSON.stringify(range.data)]);
  const symbols = await scenario(ctx, 'localFetch', surface, { label: 'read.symbols', query: { path: model, view: 'symbols' }, checks, responseChars: 500 });
  const functions = symbols.data.flatMap(data => (data.content ?? '').split('\n')).filter(line => /\bexport (function|const invoke)\b/.test(line));
  const source = sourceText(ctx, 'code/model.ts').split('\n');
  equal(checks, `read.declarations.${surface}`, functions, [`14-16\t${source[13]}`, `18-21\t${source[17]}`, `25\t${source[24]}`]);
  checks.push([`read.symbols-view.${surface}`, symbols.data.every(data => data.view === 'symbols'), JSON.stringify(symbols.data)]);
}

async function orderedRanges(ctx, surface, checks) {
  const expected = sourcePairs(sourceText(ctx, 'read/windows.txt')).filter(([line]) => line === 4);
  for (const { form, extra } of [{ form: 'content', extra: {} }, { form: 'compact', extra: { view: 'compact' } }, { form: 'matches', extra: { matchString: ['alpha'] } }]) {
    const query = { path: windows, ranges: [{ startLine: 4, endLine: 2 }], ...extra };
    const rejected = await ctx.call(ctx.q(query), surface);
    const error = rejected.json?.results?.[0]?.data ?? rejected.json;
    checks.push([`read.reversed-range.${form}.${surface}`, error?.errorCode === 'invalidInput'
      && ['ranges', 'startLine', 'endLine'].every(field => error.error.includes(field)), JSON.stringify(error)]);
    query.ranges[0].endLine = 4;
    const repaired = await scenario(ctx, 'localFetch', surface, { label: `read.repaired-range.${form}`, query, checks });
    const evidence = form === 'compact' ? expected.map(([line, text]) => [line, text.replace(/\r$/, '')]) : expected;
    orderedEqual(checks, `read.repaired-range.${form}.${surface}`, pairs(repaired.data), evidence);
  }
  await mixedRangeBatch(ctx, surface, checks, expected);
}

async function mixedRangeBatch(ctx, surface, checks, expected) {
  const result = await ctx.call({ queries: [
    { path: windows, ranges: [{ startLine: 4, endLine: 2 }] },
    { path: windows, inventedField: true },
    { path: windows, ranges: [{ startLine: 4, endLine: 4 }] },
  ] }, surface);
  const rows = result.json?.results ?? [];
  checks.push([`read.invalid-bounds-isolated.${surface}`, rows.length === 3
    && rows[0].index === 0 && rows[0].status === 'error' && rows[0].data.error.includes('endLine')
    && rows[1].index === 1 && rows[1].status === 'error' && rows[1].data.error.includes('inventedField')
    && rows[2].index === 2 && !rows[2].status, JSON.stringify(result.json)]);
  orderedEqual(checks, `read.valid-sibling-evidence.${surface}`, pairs([rows[2]?.data ?? {}]), expected);
}

export async function advancedRead(ctx) {
  const checks = [];
  const longPath = fixturePath('read/complete-unicode.txt');
  const source = Array.from({ length: 240 }, (_, i) => `row ${i + 1}: 界😀 source evidence ${'x'.repeat(40)}`).join('\n');
  fs.writeFileSync(path.join(ctx.cwd, longPath), source);
  for (const surface of ctx.surfaces) {
    await selections(ctx, surface, checks);
    await views(ctx, surface, checks);
    await orderedRanges(ctx, surface, checks);
    const walked = await scenario(ctx, 'localFetch', surface, { label: 'read.unicode-walk', query: { path: longPath }, checks, responseChars: 800 });
    orderedEqual(checks, `read.exact-once-utf8.${surface}`, pairs(walked.data), sourcePairs(source));
    checks.push([`read.exercised-continuation.${surface}`, walked.pages > 1, `${walked.pages} pages`]);
  }
  return checks;
}
