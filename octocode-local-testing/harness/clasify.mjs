// clasify navigation: locate answers inside large unread files (ground truth
// from rg, never from the model), negative targets, multi-target matrices,
// Scout over search results, Judge on held state, and error handling.
// Measures host-visible bytes (clasify output + verification read) against a
// direct read of the file.
import fs from 'node:fs';
import path from 'node:path';
import { REPOS, checks, collect, rowData, startServer, writeResults } from './mcp-client.mjs';

const { check, summary } = checks('clasify');
const client = await startServer();
const { raw, call } = client;
const MAX_CALLS = 45;
// Required on every clasify matrix; sent with each question and the evidence state.
const GOAL = 'Find the source that answers the harness target so the host reads only that range.';

/** The first line of `dir/file` matching `regex` (ground truth never comes from the model). */
function groundTruth(dir, file, regex) {
  const full = path.join(REPOS, dir, file);
  if (!fs.existsSync(full)) return null;
  const re = new RegExp(regex);
  const index = fs.readFileSync(full, 'utf8').split('\n').findIndex(line => re.test(line));
  return index < 0 ? null : { file: full, line: index + 1 };
}

/** Run a matrix, following next.clasify; returns every page and total bytes. */
async function locateAll(matrix, stopWhen) {
  let request = matrix;
  const pages = [];
  const bests = [];
  let bytes = 0;
  let calls = 0;
  let last;
  while (request && calls < MAX_CALLS) {
    calls += 1;
    last = await raw('clasify', { queries: [request] });
    bytes += last.bytes;
    const q = last.sc?.queries?.[0] ?? last.sc?.results?.[0]?.data?.queries?.[0];
    for (const resource of q?.resources ?? []) for (const page of resource.pages ?? []) pages.push({ resource: resource.resourceId, ...page });
    if (q?.best) bests.push(q.best);
    if (last.isError || !q) break;
    if (stopWhen?.(pages)) break;
    request = q.next?.clasify;
  }
  return { pages, bests, bytes, calls, last };
}

const LOCATE = [
  { name: 'redis serverCron', dir: 'c', glob: 'src/server.c', regex: '^int serverCron\\(', target: 'The periodic timer function that runs background housekeeping tasks many times per second.' },
  { name: 'django QuerySet._fetch_all', dir: 'python', glob: 'django/db/models/query.py', regex: 'def _fetch_all\\(', target: 'The method that evaluates the query and fills the result cache.' },
  { name: 'tokio bounded channel()', dir: 'rust', glob: 'tokio/src/sync/mpsc/bounded.rs', regex: '^pub fn channel<', target: 'The public function that creates a bounded multi-producer channel with a given buffer capacity.' },
  { name: 'prometheus tsdb Open', dir: 'go', glob: 'tsdb/db.go', regex: '^func Open\\(', target: 'The exported function that opens an existing database directory or creates a new one.' },
  { name: 'guava checkArgument', dir: 'java', glob: 'guava/src/com/google/common/base/Preconditions.java', regex: 'public static void checkArgument\\(boolean expression\\)', target: 'The method that throws IllegalArgumentException when a boolean expression is false, without a message.' },
  { name: 'TS binder bindWorker', dir: 'typescript', glob: 'tsc/testdata/fixtures/compiler/binder.ts', regex: 'function bindWorker\\(node: Node\\)', target: 'The function that dispatches binding by syntax kind for a single node.' },
  { name: 'excalidraw handleCanvasPointerDown', dir: 'tsx', glob: 'packages/excalidraw/components/App.tsx', regex: 'private handleCanvasPointerDown = \\(', target: 'The handler that runs when the pointer is pressed down on the drawing canvas.' },
  { name: 'lodash debounce', dir: 'javascript', glob: 'lodash.js', regex: 'function debounce\\(func, wait, options\\)', target: 'The function that delays invoking a function until a wait time has elapsed since the last call.' },
  { name: 'nlohmann get_bson_cstr', dir: 'cpp', glob: 'include/nlohmann/detail/input/binary_reader.hpp', regex: 'bool get_bson_cstr\\(', target: 'The method that reads a NUL-terminated C string from BSON input.' },
  { name: 'Newtonsoft ResolveTypeName', dir: 'csharp', glob: 'Src/Newtonsoft.Json/Serialization/JsonSerializerInternalReader.cs', regex: 'private void ResolveTypeName\\(', target: 'The method that resolves a $type type name from JSON into a CLR type during deserialization.' },
  { name: 'cats Chain.deleteFirst', dir: 'scala', glob: 'core/src/main/scala/cats/data/Chain.scala', regex: 'final def deleteFirst\\(', target: 'The method that removes the first element matching a predicate and returns it with the remaining chain.' },
  { name: 'libjpeg-turbo huff encode (asm)', dir: 'asm', glob: 'simd/i386/jchuff-sse2.asm', regex: '^EXTN\\(jsimd_huff_encode_one_block_sse2\\):', target: 'The entry label of the SSE2 routine that Huffman-encodes one block of coefficients.' },
  { name: 'linux __schedule', dir: 'huge-c', glob: 'kernel/sched/core.c', regex: '__schedule\\(int sched_mode\\)', target: 'The main scheduler function that picks the next task to run and switches context.' },
  { name: 'kubernetes scheduleOnePod', dir: 'huge-go', glob: 'pkg/scheduler/schedule_one.go', regex: 'func \\(sched \\*Scheduler\\) scheduleOnePod\\(', target: 'The scheduler method that runs the scheduling cycle and then the binding cycle for a single queued pod.' },
  { name: 'vscode TextModel.applyEdits', dir: 'huge-ts', glob: 'src/vs/editor/common/model/textModel.ts', regex: '^\\tpublic applyEdits\\(', target: 'The public method that applies a list of edit operations to the text model.' },
  { name: 'rustc mir_borrowck', dir: 'huge-rust', glob: 'compiler/rustc_borrowck/src/lib.rs', regex: '^fn mir_borrowck\\(', target: 'The query provider function that runs MIR borrow checking for a body.' },
  { name: 'elasticsearch InternalEngine.index', dir: 'huge-java', glob: 'server/src/main/java/org/elasticsearch/index/engine/InternalEngine.java', regex: 'public IndexResult index\\(Index index\\)', target: 'The engine method that indexes one document operation.' },
  { name: 'pytorch autograd Engine::execute', dir: 'huge-cpp', glob: 'torch/csrc/autograd/engine.cpp', regex: '^auto Engine::execute\\(', target: 'The autograd engine method that executes the backward graph from given root edges.' },
];

const table = [];
for (const t of LOCATE) {
  const truth = groundTruth(t.dir, t.glob, t.regex);
  if (!check(`${t.name}: ground truth found`, truth, `${t.dir}/${t.glob}`)) continue;
  const size = fs.statSync(truth.file).size;
  const started = Date.now();
  const run = await locateAll({
    goal: GOAL, reasoning: 'Locate one implementation before reading a large file.',
    resources: [{ id: 'file', context: { tool: 'localFetch', query: { reasoning: 'unread large file', path: truth.file, fullContent: true } } }],
    questions: [{ id: 't', questionType: 'locate', target: t.target }],
  });
  // Every returned window (a page may carry a near-tie runner-up), ranked the
  // way the server's `best` ranks them: exists, then probability.
  const ranked = run.pages
    .flatMap(p => (p.answers?.t?.matches ?? []).map(m => ({ answer: { exists: p.answers.t.exists, matches: [m] } })))
    .sort((a, b) => b.answer.exists - a.answer.exists || b.answer.matches[0].probability - a.answer.matches[0].probability);
  const best = ranked[0];
  const window = best?.answer.matches[0];
  // Strict: the rank-1 window itself shows a declaration line (overloads count).
  const declRe = new RegExp(t.regex);
  const fileLines = fs.readFileSync(truth.file, 'utf8').split('\n');
  const strict = !!window && fileLines.slice(window.startLine - 1, window.endLine).some(line => declRe.test(line));
  const runnerUps = run.pages.filter(p => (p.answers?.t?.matches ?? []).length > 1).length;
  const truthText = fs.readFileSync(truth.file, 'utf8').split('\n')[truth.line - 1].trim().slice(0, 40);
  // The skill's read step: batch the top windows (≤5 ranges); here the top 3.
  let verifyBytes = 0;
  let verifiedRank = -1;
  let viaDoc = false;
  const srcLines = fs.readFileSync(truth.file, 'utf8').split('\n');
  let docStart = truth.line;
  while (docStart > 1 && /^\s*(\/\*\*?|\*|\/\/|#|;|@)/.test(srcLines[docStart - 2])) docStart -= 1;
  for (const [rank, candidate] of ranked.slice(0, 3).entries()) {
    const m = candidate.answer.matches[0];
    const read = await call('localFetch', { path: truth.file, startLine: Math.max(1, m.startLine - 2), endLine: m.endLine + 2 });
    verifyBytes += read.bytes;
    // A window inside the target's own leading doc block is the right place;
    // the agent widens to the declaration below it (counted, labelled "doc").
    const hasDecl = (rowData(read)?.content ?? '').includes(truthText);
    const inDoc = !hasDecl && m.endLine >= docStart && m.startLine < truth.line;
    if (verifiedRank < 0 && (hasDecl || inDoc)) { verifiedRank = rank; viaDoc = inDoc; }
  }
  const truthRank = verifiedRank;
  const top1 = verifiedRank === 0;
  const hostBytes = run.bytes + verifyBytes;
  table.push({
    target: t.name, fileKB: Math.round(size / 1024), calls: run.calls, pages: run.pages.length,
    truth: truth.line, best: window ? `${window.startLine}-${window.endLine}` : '-', exists: best?.answer.exists, rank: (verifiedRank + 1 || 'miss') + (viaDoc ? ' doc' : ''), top1, strict, runnerUps,
    hostKB: (hostBytes / 1024).toFixed(1), saving: `${Math.round(100 - (100 * hostBytes) / size)}%`, s: ((Date.now() - started) / 1000).toFixed(0),
  });
  check(`${t.name}: the declaration is in the top-3 windows read (rank ${verifiedRank + 1})`, verifiedRank >= 0, `best=${window?.startLine}-${window?.endLine} exists=${best?.answer.exists} truth=${truth.line}`);
  // Admission: small files are read directly (skill); only assert leverage ≥64KB.
  if (size >= 64 * 1024) check(`${t.name}: host bytes < 10% of a full read`, hostBytes < size * 0.1, `${hostBytes}B vs ${size}B`);
}
console.table(table);
const top1 = table.filter(r => r.top1).length;
console.log(`locate top-1 exact: ${top1}/${table.length}`);
console.log(`locate strict (declaration line inside the rank-1 window): ${table.filter(r => r.strict).length}/${table.length}`);

// Server-side prefilter: keep first-page quality visible, then follow the
// bounded partial-result contract until the ranked answer can be verified.
let prefilterQuality;
{
  const truth = groundTruth('typescript', 'tsc/testdata/fixtures/compiler/checker.ts', 'function isTypeAssignableTo\\(');
  const matrix = {
    goal: GOAL, reasoning: 'prefilter', resources: [{ id: 'f', prefilter: ['assignable'], context: { tool: 'localFetch', query: { reasoning: 'unread', path: truth.file, fullContent: true } } }],
    questions: [{ id: 't', questionType: 'locate', target: 'The function that checks whether a source type is assignable to a target type.' }],
  };
  const out = await raw('clasify', { queries: [matrix] });
  const q = out.sc?.queries?.[0];
  const windows = (q?.resources?.[0]?.pages ?? []).flatMap(p => (p.answers?.t?.matches ?? []).map(m => ({ exists: p.answers.t.exists, ...m })))
    .sort((a, b) => b.exists - a.exists || b.probability - a.probability);
  const top = windows[0];
  const lines = fs.readFileSync(truth.file, 'utf8').split('\n');
  const strict = !!top && lines.slice(top.startLine - 1, top.endLine).some(l => /function isTypeAssignableTo\(/.test(l));
  prefilterQuality = { firstPageRankOneDeclaration: strict, firstPageComplete: !q?.next, firstPageBytes: out.bytes };
  let current = q, last = out, calls = 1, workflowBytes = out.bytes;
  const rankedDeclaration = row => row?.best?.t?.[0];
  const containsDeclaration = row => {
    const window = rankedDeclaration(row);
    return !!window && lines.slice(window.startLine - 1, window.endLine).some(l => /function isTypeAssignableTo\(/.test(l));
  };
  while (!containsDeclaration(current) && current?.next?.clasify && calls < MAX_CALLS && !last.isError) {
    last = await raw('clasify', { queries: [current.next.clasify] });
    workflowBytes += last.bytes; calls += 1;
    current = last.sc?.queries?.[0];
  }
  const read = rankedDeclaration(current)?.next?.read;
  let verified = false;
  if (containsDeclaration(current) && read?.tool === 'localFetch') {
    const verification = await raw(read.tool, { queries: [read.query] });
    workflowBytes += verification.bytes;
    verified = !verification.isError && collect(verification.sc, o => typeof o.content === 'string').some(o => /function isTypeAssignableTo\(/.test(o.content));
  }
  Object.assign(prefilterQuality, { calls, workflowBytes, verified, terminal: !current?.next });
  check('prefilter: bounded continuations reach the rank-1 declaration and its exact read verifies it', containsDeclaration(current) && verified && !last.isError, JSON.stringify(prefilterQuality));
  check('prefilter: first-page host bytes under 0.5% of the file', out.bytes < fs.statSync(truth.file).size * 0.005, `${out.bytes}B; full workflow ${workflowBytes}B`);
}

// Carried best: across a multi-call walk, the final call's best is file-wide.
{
  const truth = groundTruth('c', 'src/server.c', '^int serverCron\\(');
  let request = { goal: GOAL, reasoning: 'carry', resources: [{ id: 'f', context: { tool: 'localFetch', query: { reasoning: 'unread', path: truth.file, fullContent: true } } }],
    questions: [{ id: 't', questionType: 'locate', target: 'The periodic timer function that runs background housekeeping tasks many times per second.' }] };
  let last, calls = 0;
  while (request && calls < 20) { calls++; const out = await raw('clasify', { queries: [request] }); last = out.sc?.queries?.[0]; request = last?.next?.clasify; }
  const top = last?.best?.t?.[0];
  check('carry: the final call\'s best[0] is the file-wide answer', calls > 1 && !!top && top.startLine <= truth.line && truth.line <= top.endLine, `calls=${calls} top=${JSON.stringify(top)} truth=${truth.line}`);
}

// Server-side ranking and routing hints.
{
  const truth = groundTruth('c', 'src/server.c', '^int serverCron\\(');
  const run = await locateAll({
    goal: GOAL, reasoning: 'server ranking',
    resources: [{ id: 'file', context: { tool: 'localFetch', query: { reasoning: 'unread', path: truth.file, fullContent: true } } }],
    questions: [{ id: 't', questionType: 'locate', target: 'The periodic timer function that runs background housekeeping tasks many times per second.' }],
  }, pages => pages.length >= 4);
  const rows = run.bests.flatMap(b => b.t ?? []);
  const ordered = rows.length >= 2 && run.bests.every(b => (b.t ?? []).every((r, i, a) => i === 0 || a[i - 1].exists > r.exists || (a[i - 1].exists === r.exists && a[i - 1].probability >= r.probability)));
  check('best: multi-page calls rank windows by exists, then probability', ordered, JSON.stringify(run.bests[0]?.t?.slice(0, 3)));
  const hinted = await raw('clasify', { queries: [{ goal: GOAL, reasoning: 'hint', resources: [{ context: { tool: 'localFetch', query: { reasoning: 'x', path: truth.file, startLine: 1, endLine: 40 } } }], questions: [{ id: 'h', questionType: 'locate', target: 'Where is serverCron defined?' }] }] });
  const q = hinted.sc?.queries?.[0];
  check('hint: an identifier target suggests localSearch', (q?.hints ?? []).some(h => h.includes('serverCron') && h.includes('localSearch')), JSON.stringify(q?.hints));
  const plain = await raw('clasify', { queries: [{ goal: GOAL, reasoning: 'hint', resources: [{ context: { tool: 'localFetch', query: { reasoning: 'x', path: truth.file, startLine: 1, endLine: 40 } } }], questions: [{ id: 'h', questionType: 'locate', target: 'The license header of this file.' }] }] });
  check('hint: a described target gets no routing hint', !plain.sc?.queries?.[0]?.hints && !plain.isError, JSON.stringify(plain.sc?.queries?.[0]?.hints ?? plain.isError));
}

// Huge single file (3MB, 54k lines): narrow first, as the skill prescribes.
// A cheap text search yields line anchors; the three densest 600-line clusters
// become bounded resources in one matrix; the top windows are then read.
{
  const truth = groundTruth('typescript', 'tsc/testdata/fixtures/compiler/checker.ts', 'function isTypeAssignableTo\\(');
  const search = await call('localSearch', { path: truth.file, searchText: 'assignable', caseMode: 'insensitive', maxMatchesPerFile: 100, matchContentLength: 20 });
  const lines = collect(rowData(search), o => typeof o.line === 'number' && typeof o.value === 'string').map(m => m.line);
  const buckets = new Map();
  for (const line of lines) buckets.set(Math.floor(line / 600), (buckets.get(Math.floor(line / 600)) ?? 0) + 1);
  const clusters = [...buckets].sort((a, b) => b[1] - a[1]).slice(0, 3).map(([b]) => ({ startLine: b * 600 + 1, endLine: b * 600 + 600 }));
  const run = await locateAll({
    goal: GOAL, reasoning: 'Locate within the densest candidate regions of a huge file.',
    resources: clusters.map((c, i) => ({ id: `r${i}`, context: { tool: 'localFetch', query: { reasoning: 'bounded section', path: truth.file, ...c } } })),
    questions: [{ id: 't', questionType: 'locate', target: 'The function that checks whether a source type is assignable to a target type.' }],
  });
  const ranked = run.pages.filter(p => p.answers?.t?.matches?.length).sort((a, b) => b.answers.t.exists - a.answers.t.exists || b.answers.t.matches[0].probability - a.answers.t.matches[0].probability);
  const found = ranked.slice(0, 3).findIndex(p => truth.line >= p.answers.t.matches[0].startLine - 3 && truth.line <= p.answers.t.matches[0].endLine + 3);
  const hostBytes = search.bytes + run.bytes;
  check(`huge file (3MB checker.ts): search → bounded clusters → locate finds line ${truth.line} (rank ${found + 1})`, found >= 0, `clusters=${JSON.stringify(clusters)} top=${JSON.stringify(ranked[0]?.answers.t)}`);
  check('huge file: narrowed flow stays under 1% of a full read', hostBytes < fs.statSync(truth.file).size * 0.01, `${hostBytes}B`);
}

// Negative: a target absent from the file must not be reported as present.
{
  const truth = groundTruth('c', 'src/server.c', '^int serverCron\\(');
  const run = await locateAll({
    goal: GOAL, reasoning: 'Absent target stays unresolved.',
    resources: [{ id: 'file', context: { tool: 'localFetch', query: { reasoning: 'unread', path: truth.file, fullContent: true } } }],
    questions: [{ id: 't', questionType: 'locate', target: 'The function that parses a YAML configuration file into nested dictionaries.' }],
  });
  const maxExists = Math.max(...run.pages.map(p => p.answers?.t?.exists ?? 0));
  check('absent target: every page exists < 0.5', maxExists < 0.5, `max exists=${maxExists} over ${run.pages.length} pages`);
}

// Matrix: two targets, one capture.
{
  const cron = groundTruth('c', 'src/server.c', '^int serverCron\\(');
  const proc = groundTruth('c', 'src/server.c', '^int processCommand\\(');
  const run = await locateAll({
    goal: GOAL, reasoning: 'Two independent facts from one capture.',
    resources: [{ id: 'file', context: { tool: 'localFetch', query: { reasoning: 'unread', path: cron.file, fullContent: true } } }],
    questions: [
      { id: 'cron', questionType: 'locate', target: 'The periodic timer function that runs background housekeeping tasks many times per second.' },
      { id: 'proc', questionType: 'locate', target: 'The function that validates a client command and decides whether to execute or queue it.' },
    ],
  });
  const bestFor = id => run.pages.map(p => ({ a: p.answers?.[id] })).filter(p => p.a?.matches?.length).sort((x, y) => y.a.exists - x.a.exists || y.a.matches[0].probability - x.a.matches[0].probability)[0]?.a.matches[0];
  // A window counts when it lies within the target function (declaration to
  // its closing brace): the answer passage may be inside the body.
  const lines = fs.readFileSync(cron.file, 'utf8').split('\n');
  const spanEnd = start => start + lines.slice(start).findIndex(l => /^}/.test(l));
  const inWindow = (w, line) => w && w.endLine >= line - 3 && w.startLine <= spanEnd(line);
  check('matrix: both targets located from one capture', inWindow(bestFor('cron'), cron.line) && inWindow(bestFor('proc'), proc.line), JSON.stringify({ cron: bestFor('cron'), cronLine: cron.line, proc: bestFor('proc'), procLine: proc.line }));
}

// Scout over search results: rank the implementing file first.
{
  const root = path.join(REPOS, 'huge-go/pkg/scheduler');
  if (fs.existsSync(root)) {
    const out = await raw('clasify', { queries: [{
      goal: GOAL, reasoning: 'Choose the file to read among search hits.',
      resources: [{ id: 'hits', context: { tool: 'localSearch', query: { reasoning: 'candidate files', path: root, searchText: 'func (sched *Scheduler)', regex: 'literal', resultView: 'files', pageSize: 20 } } }],
      questions: [{ id: 'core', type: 'noul', instructions: 'Is this file the implementation of scheduling one pod (the main per-pod scheduling cycle)?' }],
    }] });
    const q = out.sc?.queries?.[0];
    const ranked = (q?.resources?.[0]?.pages ?? []).map(p => ({ path: p.source?.path ?? '', p: p.answers?.core?.noul ?? p.answers?.core })).sort((a, b) => b.p - a.p);
    check('scout: schedule_one.go ranks first among search hits', ranked[0]?.path.endsWith('schedule_one.go'), ranked.slice(0, 3).map(r => `${path.basename(r.path)}=${r.p}`).join(', '));
  }
}

// Judge: held snippets with known answers.
{
  const cases = [
    { value: 'func sum(n int) int { total := 0; for i := 0; i < n; i++ { total += i }; return total }', q: 'Does this code contain a loop?', yes: true },
    { value: 'def area(r):\n    return 3.14159 * r * r', q: 'Does this function perform any I/O (files, network, or printing)?', yes: false },
    { value: 'fn push(v: &mut Vec<i32>, x: i32) { v.push(x); }', q: 'Does this function mutate one of its arguments?', yes: true },
    { value: 'const add = (a, b) => a + b;', q: 'Does this code catch or handle exceptions?', yes: false },
  ];
  const out = await raw('clasify', { queries: [{
    goal: GOAL, reasoning: 'Held-state calibration.',
    resources: cases.map((c, i) => ({ id: `c${i}`, context: { value: c.value } })),
    questions: cases.map((c, i) => ({ id: `q${i}`, type: 'noul', instructions: c.q })),
  }] });
  const resources = out.sc?.queries?.[0]?.resources ?? [];
  let correct = 0;
  cases.forEach((c, i) => {
    const p = resources.find(r => r.resourceId === `c${i}`)?.pages?.[0]?.answers?.[`q${i}`]?.noul;
    if (typeof p === 'number' && (c.yes ? p >= 0.8 : p <= 0.2)) correct += 1;
  });
  check(`judge: ${correct}/${cases.length} held snippets answered decisively and correctly`, correct === cases.length, out.text.slice(0, 200));
}

// Errors: missing and binary files stay unresolved, never "no".
{
  const missing = await raw('clasify', { queries: [{ goal: GOAL, reasoning: 'missing', resources: [{ context: { tool: 'localFetch', query: { reasoning: 'x', path: path.join(REPOS, 'c/nope.c'), fullContent: true } } }], questions: [{ id: 't', questionType: 'locate', target: 'anything' }] }] });
  check('error: missing file is reported, not judged', /error|not found|File not found/i.test(missing.text) && !/"exists":0\./.test(missing.text), missing.text.slice(0, 160));
}

const result = summary();
writeResults('clasify', { table, prefilterQuality, ...result });
client.close();
process.exitCode = result.failed.length ? 1 : 0;
