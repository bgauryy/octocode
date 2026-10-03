// Large-file probes plus complete walks where the checks require exhaustion.
// Verifies: no crash/timeout, bounded response size, gap-free page coverage,
// totals that agree across pages, and executable continuations.
import fs from 'node:fs';
import path from 'node:path';
import { FIXTURES, REPOS, checks, collect, declarations, nextHints, rowData, sourceView, startServer, structureFiles, writeResults, lspLocations } from './mcp-client.mjs';

const { check, summary } = checks('large-files');
const MAX_RESPONSE_BYTES = 2_000_000;
const R = (p) => path.join(REPOS, p);
const TARGETS = {
  checkerTs: R('typescript/tsc/testdata/fixtures/compiler/checker.ts'),
  libDom: R('typescript/tsc/internal/bundled/libs/lib.dom.d.ts'),
  checkerGo: R('typescript/tsc/internal/checker/checker.go'),
  jsonHpp: R('cpp/single_include/nlohmann/json.hpp'),
  oneLineWasm: R('tsx/packages/excalidraw/subset/woff2/woff2-wasm.ts'),
  moduleC: R('c/src/module.c'),
};

// Synthetic worst cases.
const synth = path.join(FIXTURES, 'large');
fs.mkdirSync(synth, { recursive: true });
const bigLog = path.join(synth, 'huge.log');
const minified = path.join(synth, 'minified.min.js');
const manyFns = path.join(synth, 'many-functions.ts');
if (!fs.existsSync(bigLog)) {
  const lines = [];
  for (let i = 1; i <= 400_000; i++) lines.push(`2026-09-27T00:00:${String(i % 60).padStart(2, '0')}Z level=${i % 97 === 0 ? 'ERROR' : 'info'} req=${i} msg="request handled in ${i % 1000}ms"`);
  fs.writeFileSync(bigLog, lines.join('\n') + '\n');
}
if (!fs.existsSync(minified)) {
  const parts = [];
  for (let i = 0; i < 60_000; i++) parts.push(`function f${i}(a){return a+${i}}`);
  fs.writeFileSync(minified, parts.join(';'));
}
if (!fs.existsSync(manyFns)) {
  const parts = [];
  for (let i = 0; i < 20_000; i++) parts.push(`export function fn${i}(value: number): number {\n  return helper(value + ${i});\n}\n`);
  parts.push('export function helper(n: number): number { return n; }\n');
  fs.writeFileSync(manyFns, parts.join(''));
}

const client = await startServer();
const { call, raw } = client;

function lines(file) { return fs.readFileSync(file, 'utf8').split('\n').length - (fs.readFileSync(file, 'utf8').endsWith('\n') ? 1 : 0); }
function bounded(entry, name) { check(`${name}: response ≤ ${MAX_RESPONSE_BYTES}B`, entry.bytes <= MAX_RESPONSE_BYTES, `${entry.bytes}B ${entry.ms}ms`); }
function hint(entry, key) { return nextHints(entry.sc).find(h => h.path.endsWith(`.${key}`)); }

/** Follow `next.<key>` until it disappears; returns the page entries. */
async function walk(first, key, maxPages, label) {
  const pages = [first];
  let current = first;
  while (pages.length < maxPages) {
    const h = hint(current, key);
    if (!h) break;
    current = await raw(h.tool, h.query, `${label} page ${pages.length + 1}`);
    if (current.isError || current.rowErrors) break;
    pages.push(current);
  }
  return pages;
}

// ── localFetch: chunked walk over the whole of each large file ──────────────
for (const [name, file, sample] of [['checkerTs', TARGETS.checkerTs, 0], ['jsonHpp', TARGETS.jsonHpp, 0], ['bigLog (28MB, streamed)', bigLog, 6]]) {
  const total = lines(file);
  const first = await call('localFetch', { path: file, chunkType: 'lines', chunkSize: 5000 }, {}, `fetch ${name}`);
  bounded(first, `localFetch ${name} page 1`);
  const pages = await walk(first, 'continue', sample || 400, `fetch ${name}`);
  const ranges = pages.map(p => { const r = sourceView(rowData(p)).ranges; return [r[0]?.start, r.at(-1)?.end]; });
  let gapFree = ranges[0]?.[0] === 1;
  for (let i = 1; i < ranges.length; i++) gapFree &&= ranges[i][0] === ranges[i - 1][1] + 1;
  const last = ranges.at(-1)?.[1];
  check(sample ? `localFetch ${name}: first ${pages.length} pages advance gap-free` : `localFetch ${name}: ${pages.length} pages cover 1..${total} gap-free`, gapFree && (sample ? pages.length === sample : last === total), `ranges=${JSON.stringify(ranges.slice(0, 3))}… last=${last} total=${total}`);
  check(`localFetch ${name}: every page bounded`, pages.every(p => p.bytes <= MAX_RESPONSE_BYTES), Math.max(...pages.map(p => p.bytes)));
}

// localFetch default call on a 3 MB file must not dump it; fullContent must be bounded or refused clearly.
for (const [label, query] of [
  ['default', { path: TARGETS.checkerTs }],
  ['fullContent', { path: TARGETS.checkerTs, fullContent: true }],
  ['minify symbols', { path: TARGETS.checkerTs, minify: 'symbols' }],
  ['matchString', { path: TARGETS.checkerTs, matchString: 'function createTypeChecker', contextLines: 3 }],
  ['matchString regex many', { path: TARGETS.checkerTs, matchString: '^\\s*function get\\w+Type\\(', matchStringIsRegex: true }],
  ['one-line wasm default', { path: TARGETS.oneLineWasm }],
  ['one-line wasm bytes', { path: TARGETS.oneLineWasm, chunkType: 'bytes', chunkSize: 4096 }],
  ['minified one line', { path: minified }],
]) {
  const e = await call('localFetch', query, {}, `fetch ${label}`);
  bounded(e, `localFetch checker ${label}`);
  const d = rowData(e);
  const boundedRefusal = d?.errorCode === 'fileTooLarge' && !!hint(e, 'continue');
  check(`localFetch ${label}: answers, or refuses with an executable continuation`, !e.isError || boundedRefusal, (d?.errorCode ?? '') + ` continue=${!!hint(e, 'continue')}`);
}

// bytes walk across the single 972 KB line: pages must be contiguous.
{
  const first = await call('localFetch', { path: TARGETS.oneLineWasm, chunkType: 'bytes', chunkSize: 16_384 }, {}, 'wasm bytes');
  const pages = await walk(first, 'continue', 100, 'wasm bytes');
  const size = fs.statSync(TARGETS.oneLineWasm).size;
  const biggest = Math.max(...pages.map(p => Buffer.byteLength(rowData(p)?.content ?? '', 'utf8')));
  const contiguous = pages.every((p, index) => {
    const request = p.args?.queries?.[0] ?? p.args;
    const pagination = rowData(p)?.pagination;
    return !p.isError && !p.rowErrors && request.chunkType === 'bytes' && request.chunkSize === 16_384 && (request.offset ?? 0) === index * 16_384 && (!pagination || (pagination.chunkType === 'bytes' && pagination.offset === index * 16_384));
  });
  check(`localFetch one-line 972KB (redacted blob) bytes walk: ${pages.length} contiguous pages, each chunk-sized`, contiguous && pages.length === Math.ceil(size / 16_384) && !hint(pages.at(-1), 'continue') && biggest > 0 && biggest <= 2 * 16_384, `pages=${pages.length} biggestPage=${biggest}B size=${size}`);
}

// ── localSearch: many hits in one huge file, walk match pages ──────────────
{
  const first = await call('localSearch', { path: TARGETS.checkerTs, searchText: 'function ', maxMatchesPerFile: 100, matchContentLength: 80, debug: true /* exact stats totals */ }, {}, 'grep checker');
  bounded(first, 'localSearch checker page 1');
  const d = rowData(first);
  const total = d?.stats?.totalOccurrences;
  const pages = await walk(first, 'nextMatchPage', 60, 'grep checker');
  const seen = pages.reduce((sum, p) => sum + collect(rowData(p), o => typeof o.line === 'number' && typeof o.value === 'string').length, 0);
  check(`localSearch checker 'function ': ${pages.length} match pages reach every hit`, seen >= (d?.stats?.matchedLines ?? Infinity), `seen=${seen} matchedLines=${d?.stats?.matchedLines} occurrences=${total}`);
}
{
  const e = await call('localSearch', { path: bigLog, searchText: 'level=ERROR', resultView: 'countMatches' }, {}, 'grep log count');
  const count = collect(rowData(e), o => typeof o.totalOccurrences === 'number')[0]?.totalOccurrences;
  check('localSearch 400k-line log: countMatches exact', count === Math.floor(400_000 / 97), `count=${count} expected=${Math.floor(400_000 / 97)} ${e.ms}ms`);
}
{
  const e = await call('localSearch', { path: minified, searchText: 'f59999', matchContentLength: 120 }, {}, 'grep minified');
  const values = collect(rowData(e), o => typeof o.value === 'string' && typeof o.line === 'number');
  check('localSearch 1.9MB single line: hit clipped to window', values.length === 1 && values[0].value.length <= 400, `len=${values[0]?.value.length} ${e.bytes}B`);
}
{
  const first = await call('localSearch', { path: REPOS, searchText: 'TODO', resultView: 'files', pageSize: 10, noIgnore: true, include: ['rust/tokio/src/**/*.rs'], excludeDir: ['.git', 'node_modules', 'target', 'build', 'dist', 'out'], debug: true /* filesMatched */ }, {}, 'grep repos files');
  bounded(first, 'localSearch ignored Rust corpus source files page 1');
  const pages = await walk(first, 'nextPage', 200, 'grep repos files');
  const files = pages.flatMap(p => (rowData(p)?.files ?? []).map(o => path.resolve(p.sc?.base ?? '/', o.path)));
  const unique = new Set(files);
  const reported = collect(rowData(first), o => typeof o.filesMatched === 'number')[0]?.filesMatched;
  const known = R('rust/tokio/src/fs/file/tests.rs');
  const complete = pages.length >= 2 && files.length > 0 && unique.has(known) && unique.size === files.length && files.length === reported && pages.every(p => !p.isError && !p.rowErrors) && !hint(pages.at(-1), 'nextPage') && rowData(first)?.stats?.capped === false;
  check(`localSearch ignored Rust corpus source: ${pages.length} pages, complete and duplicate-free`, complete, `known=${unique.has(known)} files=${files.length} unique=${unique.size} filesMatched=${reported} ${first.ms}ms`);
}

// ── astSearch on huge files ────────────────────────────────────────────────
{
  const sym = await call('astSearch', { operation: 'symbols', path: TARGETS.checkerTs }, {}, 'symbols checker');
  bounded(sym, 'astSearch symbols checker.ts');
  const d = rowData(sym);
  check('astSearch symbols 3MB checker.ts: answers or refuses with a repair', !sym.isError && (d?.declarations?.length > 0 || nextHints(sym.sc).length > 0 || (d?.hints?.length ?? 0) > 0), `${d?.errorCode ?? ''} decls=${d?.declarations?.length} hints=${JSON.stringify(d?.hints ?? []).slice(0, 120)}`);
}
{
  const first = await call('astSearch', { operation: 'symbols', path: manyFns, pageSize: 100 }, {}, 'symbols manyFns');
  bounded(first, 'astSearch symbols 20k functions');
  const pages = await walk(first, 'nextPage', 250, 'symbols manyFns');
  const names = pages.flatMap(p => declarations(p).filter(o => typeof o.name === 'string' && typeof o.line === 'number').map(o => o.name));
  check(`astSearch symbols 20,001 functions: ${pages.length} pages, all unique`, names.length === 20_001 && new Set(names).size === names.length, `names=${names.length}`);
}
{
  const first = await call('astSearch', { operation: 'match', path: TARGETS.checkerTs, langType: 'TypeScript', pattern: 'isTypeAssignableTo($A, $B)', maxMatchesPerFile: 50, captureText: true, debug: true /* totalStructuralMatches */ }, {}, 'match checker');
  bounded(first, 'astSearch match checker');
  const d = rowData(first);
  const total = collect(d, o => typeof o.totalStructuralMatches === 'number')[0]?.totalStructuralMatches;
  const pages = await walk(first, 'nextMatchPage', 100, 'match checker');
  const seen = pages.reduce((sum, p) => sum + collect(rowData(p), o => typeof o.value === 'string' && typeof o.column === 'number' && o.metavarRanges).length, 0);
  check(`astSearch match in 3MB file: ${pages.length} pages reach all ${total} matches`, seen === total, `seen=${seen} total=${total} ${first.ms}ms`);
}
{
  const first = await call('astSearch', { operation: 'syntaxTree', path: TARGETS.jsonHpp, namedOnly: true, nodeLimit: 150 }, {}, 'tree json.hpp');
  bounded(first, 'astSearch syntaxTree json.hpp page 1');
  const pages = await walk(first, 'nextPage', 5, 'tree json.hpp');
  check(`astSearch syntaxTree 1.2MB header: paged (${pages.length} pages sampled)`, !first.isError && (pages.length > 1 || !!hint(first, 'nextPage')), `${first.ms}ms ${first.bytes}B keys=${nextHints(first.sc).map(h => h.path).join(',').slice(0, 120)}`);
}
{
  const e = await call('astSearch', { operation: 'match', path: minified, langType: 'JavaScript', pattern: 'function $F($A) { return $$$B }', resultView: 'countMatches', debug: true /* totalStructuralMatches */ }, {}, 'match minified');
  const total = collect(rowData(e), o => typeof o.totalStructuralMatches === 'number')[0]?.totalStructuralMatches;
  check('astSearch match 60k functions on one line', total === 60_000, JSON.stringify({ total, ms: e.ms, data: rowData(e) }));
}

// ── lspSearch on a 3 MB file: documentSymbols + references paging ──────────
{
  const first = await call('lspSearch', { uri: manyFns, operation: 'documentSymbols', pageSize: 100 }, {}, 'lsp docsyms');
  bounded(first, 'lspSearch documentSymbols 20k functions');
  const pages = await walk(first, 'nextPage', 5, 'lsp docsyms');
  const names = pages.flatMap(p => collect(rowData(p)?.payload, o => typeof o.name === 'string').map(o => o.name));
  check(`lspSearch documentSymbols 20,001 fns: ${pages.length} pages advance`, pages.length === 5 && pages.every(p => !p.isError && !p.rowErrors) && new Set(names).size === names.length, `unique=${new Set(names).size}/${names.length} first=${first.ms}ms`);
  const refs = await call('lspSearch', { uri: manyFns, symbolName: 'helper', lineHint: 60_001, operation: 'references', pageSize: 100 }, {}, 'lsp refs helper');
  bounded(refs, 'lspSearch references 20k');
  const rp = await walk(refs, 'nextPage', 3, 'lsp refs helper');
  const locs = rp.reduce((sum, p) => sum + lspLocations(p).length, 0);
  check(`lspSearch references (20,001 refs): pages 1-3 follow their snapshot`, rp.length === 3 && rp.every(p => !p.isError && !p.rowErrors) && locs === 300, `locations=${locs} perPage=${rp.map(p => p.ms + 'ms').join('/')}`);
}

// ── structureSearch over the whole clone set ───────────────────────────────
{
  const first = await call('structureSearch', { operation: 'files', path: REPOS, extensions: ['ts', 'go', 'py', 'rs', 'java', 'c', 'cpp', 'hpp', 'cs', 'scala', 'asm'], detail: 'full', sort: 'size', limit: 20, defaultExcludes: false, excludeDir: ['.git', 'node_modules', 'target'] }, {}, 'biggest files'); // repos/* is gitignored
  bounded(first, 'structureSearch biggest files');
  const files = structureFiles(rowData(first)?.files).filter(o => typeof o.path === 'string' && typeof o.size === 'number');
  const sorted = files.every((f, i) => i === 0 || files[i - 1].size >= f.size);
  check('structureSearch files sort:size over 810MB', files.length === 20 && sorted, `${first.ms}ms top=${files[0]?.path} ${files[0]?.size}`);
  const tree = await call('structureSearch', { operation: 'tree', path: path.join(REPOS, 'typescript'), maxDepth: 3, pageSize: 100 }, {}, 'tree ts');
  const pages = await walk(tree, 'nextPage', 200, 'tree ts');
  const entries = pages.flatMap(p => collect(rowData(p), o => typeof o.path === 'string' || typeof o.name === 'string'));
  check(`structureSearch tree TypeScript repo depth 3: ${pages.length} pages`, !tree.isError && pages.length >= 1, `entries≈${entries.length} ${tree.ms}ms`);
}

// ── whole-response pagination: reassemble a big bulk response exactly ──────
{
  const args = { queries: [{ goal: 'Verify response pagination over real AST evidence', reasoning: 'Read a large declaration page', operation: 'symbols', path: manyFns, pageSize: 1000 }] };
  const full = await raw('astSearch', { ...args, responseCharLength: 50000 }, 'response full');
  const firstPage = await raw('astSearch', { ...args, responseCharLength: 4000 }, 'response page 1');
  let text = firstPage.text.replace(/^# Response page[^\n]*\n/, '');
  let current = firstPage;
  let pages = 1;
  while (current.sc?.responsePagination?.next && pages < 500) {
    const n = current.sc.responsePagination.next;
    current = await raw(n.tool, n.query, `bulk page ${++pages}`);
    text += current.text.replace(/^# Response page[^\n]*\n/, '');
  }
  check(`response pagination: ${pages} char pages reassemble the full response`, !full.isError && !full.rowErrors && !firstPage.isError && !firstPage.rowErrors && !current.isError && !current.rowErrors && pages > 1 && !current.sc?.responsePagination?.next && text.trim() === full.text.replace(/^# Response page[^\n]*\n/, '').trim(), `full=${full.text.length} rebuilt=${text.length}`);
}

const result = summary();
writeResults('large-files', { ...result, calls: client.log.map(({ text, sc, ...meta }) => meta) });
client.close();
process.exitCode = result.failed.length ? 1 : 0;
