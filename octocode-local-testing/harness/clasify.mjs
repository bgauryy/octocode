// clasify navigation: locate answers inside large unread files (ground truth
// from rg, never from the model), negative targets, multi-target matrices,
// Scout over search results, Judge on held state, and error handling.
// Measures host-visible bytes (clasify output + verification read) against a
// direct read of the file. Scout flows (every scoutTools list/fetch resource:
// candidate mapping, next.clasify, hints.read, errors) run in `scoutFlows`;
// OCTOCODE_CLASIFY_ONLY=scout runs only them.
import fs from 'node:fs';
import path from 'node:path';
import { REPOS, checks, collect, rowData, startServer, writeResults } from './mcp-client.mjs';

const { check, skip, summary } = checks('clasify');
const client = await startServer();
const { raw, call, follow } = client;
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

/** A locate row with its window as `startLine`/`endLine` (the row carries `line`/`endLine`). */
const row = r => r && (r.line ? { ...r, startLine: r.line } : r);
/** Ranked locate rows of one question in a query result (server order). */
const bestRows = (q, id) => (q?.best?.[id] ?? []).map(row);
/** A page's answer to question `id`: compact bare value or the debug object. */
const pageExists = (p, id) => (typeof p.answers?.[id] === 'number' ? p.answers[id] : p.answers?.[id]?.exists ?? 0);
/** Resource-level or single-page answer of a judged supplied value. */
const answerOf = (resource, id) => {
  const answer = resource?.answers?.[id] ?? resource?.pages?.[0]?.answers?.[id];
  return typeof answer === 'object' && answer !== null ? answer.yesno ?? answer : answer;
};
/** The server's ranking: exists first, then window probability. */
const rankOrder = rows => [...rows].sort((a, b) => b.exists - a.exists || b.probability - a.probability);

/** Run a matrix, following next.clasify; returns every page and total bytes. */
async function locateAll(matrix, stopWhen) {
  let request = { queries: [matrix] };
  const pages = [];
  const bests = [];
  let bytes = 0;
  let calls = 0;
  let last, final;
  while (request && calls < MAX_CALLS) {
    calls += 1;
    last = await raw('clasify', request);
    bytes += last.bytes;
    const q = last.sc?.queries?.[0] ?? last.sc?.results?.[0]?.data?.queries?.[0];
    for (const resource of q?.resources ?? []) for (const page of resource.pages ?? []) pages.push({ resource: resource.id, path: page.path ?? page.source?.path ?? resource.path, ...page });
    if (q?.best) bests.push(q.best);
    final = q;
    if (last.isError || !q) break;
    if (stopWhen?.(pages)) break;
    request = q.next?.clasify;
  }
  return { pages, bests, bytes, calls, last, final };
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

/** One Scout page's candidate identity: file path or list item. */
const pageId = (resource, page) => page.path ?? page.source?.item ?? page.source?.path ?? resource.path ?? '';
/** A clasify call's first matrix and first resource. */
const firstResource = out => out.sc?.queries?.[0]?.resources?.[0];

/**
 * Scout flows over every scoutTools resource: a list's candidates each become
 * exactly one page (no drop, no repeat across next.clasify), every page's
 * hints.read runs verbatim on that candidate, path lists stay one page for a
 * choice, fetches judge with `sufficient`, and each misuse fails with a clear
 * errorCode plus the read tool's own recovery.
 */
async function scoutFlows() {
  const tokio = path.join(REPOS, 'rust/tokio');
  if (!fs.existsSync(tokio)) { skip('scout flows', `missing ${tokio}`); return; }
  const listed = name => client.tools.some(tool => tool.name === name);
  const github = listed('ghSearchRepo') && process.env.OCTOCODE_TEST_SCOUT_GITHUB !== '0';
  const ask = 'Where an idle blocking-pool thread exits after its keep-alive timeout';
  const screen = [{ id: 'rel', type: 'relevant', ask }, { id: 'suf', type: 'sufficient', ask }];
  const scout = (tool, query, questions = screen, extra = {}) => raw('clasify', { queries: [{ mainGoal: GOAL, resources: [{ id: 'r', tool, query, ...extra }], questions }] });
  // Candidate identities of a direct list page, as Scout names them.
  const LISTS = [
    { tool: 'localSearch', query: { path: path.join(tokio, 'src'), matchString: 'keep_alive', pageSize: 5 }, ids: d => d.files?.map(f => f.path) },
    { tool: 'astSearch', query: { path: path.join(tokio, 'src/runtime'), operation: 'match', pattern: 'Duration::from_secs($A)', language: 'rust', pageSize: 10 }, ids: d => d.files?.map(f => f.path) },
    { tool: 'astSearch', query: { path: path.join(tokio, 'src/runtime/blocking'), operation: 'symbols' }, ids: d => d.files?.map(f => f.path) },
    { tool: 'lspSearch', query: { path: path.join(tokio, 'src/runtime/blocking/pool.rs'), operation: 'references', symbolName: 'spawn_blocking', lineHint: 238 }, ids: d => d.payload?.files?.map(f => f.path) },
    ...(github ? [
      { tool: 'ghSearchCode', query: { owner: 'tokio-rs', repo: 'tokio', keywords: ['keep_alive', 'blocking'], pageSize: 5 }, ids: d => d.files?.map(f => f.path) },
      { tool: 'ghSearchRepo', query: { keywords: ['tokio', 'runtime'], pageSize: 5 }, ids: d => d.repositories?.map(r => `${r.owner}/${r.repo}`), questions: [{ id: 'is', type: 'yesno', ask: 'This repository is the tokio async runtime itself' }] },
      { tool: 'ghSearchHistory', query: { operation: 'pullRequest', owner: 'tokio-rs', repo: 'tokio', keywords: ['blocking', 'keep_alive'], pageSize: 5 }, ids: d => d.pullRequests?.map(p => `#${p.number}`) },
      { tool: 'artifactSearch', query: { ecosystem: 'npm', keywords: ['json', 'schema', 'validator'], pageSize: 5 }, ids: d => d.artifacts?.map(a => `:${a.name}`) },
    ] : []),
  ];
  for (const t of LISTS) {
    const label = `scout ${t.tool}${t.query.operation ? ` ${t.query.operation}` : ''}`;
    // A cold language server answers `timeout` with next.retry: follow it as an agent would.
    let direct = await call(t.tool, t.query);
    for (let tries = 0; tries < 4 && rowData(direct)?.errorCode === 'timeout' && rowData(direct)?.next?.retry; tries += 1) {
      direct = await follow(rowData(direct).next.retry);
    }
    const want = t.ids(rowData(direct) ?? {}) ?? [];
    if (!check(`${label}: direct list has candidates`, !direct.isError && want.length > 0, direct.text.slice(0, 160))) continue;
    const first = await scout(t.tool, t.query, t.questions);
    const resource = firstResource(first);
    const pages = resource?.pages ?? [];
    const got = pages.map(p => pageId(resource, p));
    check(`${label}: executes with answers and hints.read on every page`, !first.isError && pages.length > 0 && pages.every(p => p.answers && p.hints?.read), first.text.slice(0, 200));
    const mapped = want.map(id => got.filter(g => g.endsWith(id)).length);
    check(`${label}: each direct candidate is exactly one page`, mapped.every(n => n === 1) && got.length === want.length, JSON.stringify({ want, got }).slice(0, 220));
    // Continuations reach new candidates only: none of call 1 repeats.
    const next = first.sc?.queries?.[0]?.next?.clasify;
    if (next && t.tool !== 'localSearch') {
      const second = await raw('clasify', next);
      const again = (firstResource(second)?.pages ?? []).map(p => pageId(firstResource(second), p)).filter(id => got.includes(id));
      check(`${label}: next.clasify judges no candidate twice`, !second.isError && again.length === 0, JSON.stringify(again).slice(0, 200));
    }
    let reads = 0;
    for (const p of pages.slice(0, 3)) {
      const read = await raw(p.hints.read.tool, p.hints.read.query);
      const id = pageId(resource, p).split(/[/#@:]/).pop();
      if (!read.isError && read.text.includes(id)) reads += 1;
    }
    check(`${label}: hints.read runs verbatim on its candidate`, reads === Math.min(3, pages.length), `${reads}/${Math.min(3, pages.length)}`);
  }
  // Path lists stay one page and suit a choice over the listed paths.
  const pick = [{ id: 'pick', type: 'choice', ask: 'Which listed file implements the blocking thread pool?', labels: { pool: 'pool.rs', other: 'another listed file', insufficient: 'not listed' } }];
  const PATHS = [
    ['structureSearch', { path: path.join(tokio, 'src/runtime/blocking'), operation: 'files' }],
    ...(github ? [['ghStructure', { owner: 'tokio-rs', repo: 'tokio', path: 'tokio/src/runtime/blocking' }]] : []),
  ];
  for (const [tool, query] of PATHS) {
    const out = await scout(tool, query, pick);
    const resource = firstResource(out);
    const choice = resource?.answers?.pick ?? resource?.pages?.[0]?.answers?.pick;
    check(`scout ${tool}: a path list is one page judged by choice`, !out.isError && (resource?.pages?.length ?? 1) === 1 && (choice === 'pool' || choice?.choice === 'pool'), out.text.slice(0, 200));
  }
  // Fetch resources: `sufficient` before a large fetch, each page with its bounded read.
  const FETCHES = [
    ['localFetch', { path: path.join(tokio, 'src/runtime/blocking/pool.rs') }],
    ...(github ? [
      ['ghGetFileContent', { owner: 'tokio-rs', repo: 'tokio', path: 'tokio/src/runtime/blocking/pool.rs' }],
      ['ghGetHistoryItem', { owner: 'tokio-rs', repo: 'tokio', operation: 'pullRequest', number: 2809 }],
    ] : []),
  ];
  for (const [tool, query] of FETCHES) {
    const out = await scout(tool, query);
    const pages = firstResource(out)?.pages ?? [];
    const top = pages.reduce((best, p) => ((p.answers?.suf ?? 0) > (best?.answers?.suf ?? -1) ? p : best), null);
    const read = top?.hints?.read && await raw(top.hints.read.tool, top.hints.read.query);
    check(`scout ${tool}: fetch judged with sufficient; its best page's hints.read runs`, !out.isError && pages.length > 0 && read && !read.isError, out.text.slice(0, 200));
  }
  // Misuse: each fails with a clear errorCode and the read tool's recovery.
  const errorOf = out => firstResource(out)?.error ?? {};
  const unsupported = await scout('ghCloneRepo', { owner: 'tokio-rs', repo: 'tokio' });
  check('scout error: a non-scout tool is rejected with the allowed list', unsupported.isError && /allowed/.test(unsupported.text) && /localSearch/.test(unsupported.text), unsupported.text.slice(0, 200));
  const locate = await scout('astSearch', LISTS[2].query, [{ id: 'l', type: 'locate', ask }]);
  check('scout error: locate over a list is classificationLocateUnsupported with a repair', errorOf(locate).errorCode === 'classificationLocateUnsupported' && errorOf(locate).hints?.text?.length > 0, locate.text.slice(0, 200));
  const empty = await scout('localSearch', { path: path.join(tokio, 'src'), matchString: 'zzz_no_such_token_octocode' });
  check('scout error: an empty list is classificationContextEmpty with the tool\'s own hint', errorOf(empty).errorCode === 'classificationContextEmpty' && !(errorOf(empty).hints?.text ?? []).some(h => h.startsWith('Run the ordinary context tool')), JSON.stringify(errorOf(empty)).slice(0, 220));
  const failed = await scout('localSearch', { path: path.join(tokio, 'no-such-dir'), matchString: 'x' });
  check('scout error: a failed read keeps its errorCode and executable recovery lead', errorOf(failed).errorCode === 'pathNotFound' && (errorOf(failed).hints?.text ?? []).some(h => h.includes('structureSearch')), JSON.stringify(errorOf(failed)).slice(0, 220));
}

if (process.env.OCTOCODE_CLASIFY_ONLY === 'scout') {
  await scoutFlows();
  const result = summary();
  writeResults('clasify-scout', result);
  client.close();
  process.exit(result.failed.length ? 1 : 0);
}

const table = [];
for (const t of LOCATE) {
  const truth = groundTruth(t.dir, t.glob, t.regex);
  if (!check(`${t.name}: ground truth found`, truth, `${t.dir}/${t.glob}`)) continue;
  const size = fs.statSync(truth.file).size;
  const started = Date.now();
  const run = await locateAll({
    mainGoal: GOAL, reasoning: 'Locate one implementation before reading a large file.',
    resources: [{ id: 'file', tool: 'localFetch', query: { path: truth.file } }],
    questions: [{ id: 't', type: 'locate', ask: t.target }],
  });
  // The final call's best is the file-wide ranking (carry merges every call).
  const ranked = bestRows(run.final, 't').map(r => ({ answer: { exists: r.exists, matches: [r] } }));
  const best = ranked[0];
  const window = best?.answer.matches[0];
  // Strict: the rank-1 window itself shows a declaration line (overloads count).
  const declRe = new RegExp(t.regex);
  const fileLines = fs.readFileSync(truth.file, 'utf8').split('\n');
  const strict = !!window && fileLines.slice(window.startLine - 1, window.endLine).some(line => declRe.test(line));
  const runnerUps = run.pages.filter(p => (p.answers?.t?.matches ?? []).length > 1).length || '-';
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
    const read = await call('localFetch', { path: truth.file, ranges: [`${Math.max(1, m.startLine - 2)}-${m.endLine + 2}`] });
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
    mainGoal: GOAL, reasoning: 'prefilter', resources: [{ id: 'f', prefilter: ['assignable'], tool: 'localFetch', query: { path: truth.file } }],
    questions: [{ id: 't', type: 'locate', ask: 'The function that checks whether a source type is assignable to a target type.' }],
  };
  const out = await raw('clasify', { queries: [matrix] });
  const q = out.sc?.queries?.[0];
  const top = bestRows(q, 't')[0];
  const lines = fs.readFileSync(truth.file, 'utf8').split('\n');
  const strict = !!top && lines.slice(top.startLine - 1, top.endLine).some(l => /function isTypeAssignableTo\(/.test(l));
  prefilterQuality = { firstPageRankOneDeclaration: strict, firstPageComplete: !q?.next, firstPageBytes: out.bytes };
  let current = q, last = out, calls = 1, workflowBytes = out.bytes;
  const rankedDeclaration = q => bestRows(q, 't')[0];
  const containsDeclaration = row => {
    const window = rankedDeclaration(row);
    return !!window && lines.slice(window.startLine - 1, window.endLine).some(l => /function isTypeAssignableTo\(/.test(l));
  };
  while (!containsDeclaration(current) && current?.next?.clasify && calls < MAX_CALLS && !last.isError) {
    last = await raw('clasify', current.next.clasify);
    workflowBytes += last.bytes; calls += 1;
    current = last.sc?.queries?.[0];
  }
  // hints.read is the exact read of the top best window.
  const read = current?.hints?.read ?? current?.best?.t?.[0]?.hints?.read;
  let verified = false;
  if (containsDeclaration(current) && read?.tool === 'localFetch') {
    const verification = await raw(read.tool, read.query);
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
  let request = { queries: [{ mainGoal: GOAL, reasoning: 'carry', resources: [{ id: 'f', tool: 'localFetch', query: { reasoning: 'unread', path: truth.file, fullContent: true } }],
    questions: [{ id: 't', type: 'locate', ask: 'The periodic timer function that runs background housekeeping tasks many times per second.' }] }] };
  let last, calls = 0;
  while (request && calls < 20) { calls++; const out = await raw('clasify', request); last = out.sc?.queries?.[0]; request = last?.next?.clasify; }
  const top = bestRows(last, 't')[0];
  check('carry: the final call\'s best[0] is the file-wide answer', calls > 1 && !!top && top.startLine <= truth.line && truth.line <= top.endLine, `calls=${calls} top=${JSON.stringify(top)} truth=${truth.line}`);
}

// Server-side ranking and routing hints.
{
  const truth = groundTruth('c', 'src/server.c', '^int serverCron\\(');
  const run = await locateAll({
    mainGoal: GOAL, reasoning: 'server ranking',
    resources: [{ id: 'file', tool: 'localFetch', query: { path: truth.file } }],
    questions: [{ id: 't', type: 'locate', ask: 'The periodic timer function that runs background housekeeping tasks many times per second.' }],
  }, pages => pages.length >= 4);
  const rows = run.bests.flatMap(b => b.t ?? []);
  const ordered = rows.length >= 2 && run.bests.every(b => {
    const shown = (b.t ?? []).map(row);
    return JSON.stringify(rankOrder(shown)) === JSON.stringify(shown);
  });
  check('best: multi-page calls rank windows by exists, then probability', ordered, JSON.stringify(run.bests[0]?.t?.slice(0, 3)));
  const hinted = await raw('clasify', { queries: [{ mainGoal: GOAL, reasoning: 'hint', resources: [{ tool: 'localFetch', query: { reasoning: 'x', path: truth.file, ranges: ['1-40'] } }], questions: [{ id: 'h', type: 'locate', ask: 'Where is serverCron defined?' }] }] });
  const q = hinted.sc?.queries?.[0];
  check('hint: an identifier target suggests localSearch', (q?.hints?.text ?? []).some(h => h.includes('serverCron') && h.includes('localSearch')), JSON.stringify(q?.hints));
  const plain = await raw('clasify', { queries: [{ mainGoal: GOAL, reasoning: 'hint', resources: [{ tool: 'localFetch', query: { reasoning: 'x', path: truth.file, ranges: ['1-40'] } }], questions: [{ id: 'h', type: 'locate', ask: 'The license header of this file.' }] }] });
  check('hint: a described target gets no routing hint', !plain.sc?.queries?.[0]?.hints?.text && !plain.sc?.queries?.[0]?.hints?.textSearch && !plain.isError, JSON.stringify(plain.sc?.queries?.[0]?.hints ?? plain.isError));
}

// Huge single file (3MB, 54k lines): narrow first, as the skill prescribes.
// A cheap text search yields line anchors; the three densest 600-line clusters
// become bounded resources in one matrix; the top windows are then read.
{
  const truth = groundTruth('typescript', 'tsc/testdata/fixtures/compiler/checker.ts', 'function isTypeAssignableTo\\(');
  const search = await call('localSearch', { path: truth.file, matchString: 'assignable', caseMode: 'insensitive', matchPageSize: 100, matchContentLength: 20 });
  const lines = collect(rowData(search), o => typeof o.line === 'number' && typeof o.value === 'string').map(m => m.line);
  const buckets = new Map();
  for (const line of lines) buckets.set(Math.floor(line / 600), (buckets.get(Math.floor(line / 600)) ?? 0) + 1);
  const clusters = [...buckets].sort((a, b) => b[1] - a[1]).slice(0, 3).map(([b]) => ({ ranges: [`${b * 600 + 1}-${b * 600 + 600}`] }));
  const run = await locateAll({
    mainGoal: GOAL, reasoning: 'Locate within the densest candidate regions of a huge file.',
    resources: clusters.map((c, i) => ({ id: `r${i}`, tool: 'localFetch', query: { path: truth.file, ...c } })),
    questions: [{ id: 't', type: 'locate', ask: 'The function that checks whether a source type is assignable to a target type.' }],
  });
  const ranked = bestRows(run.final, 't');
  const found = ranked.slice(0, 3).findIndex(w => truth.line >= w.startLine - 3 && truth.line <= w.endLine + 3);
  const hostBytes = search.bytes + run.bytes;
  check(`huge file (3MB checker.ts): search → bounded clusters → locate finds line ${truth.line} (rank ${found + 1})`, found >= 0, `clusters=${JSON.stringify(clusters)} top=${JSON.stringify(ranked[0])}`);
  check('huge file: narrowed flow stays under 1% of a full read', hostBytes < fs.statSync(truth.file).size * 0.01, `${hostBytes}B`);
}

// Negative: a target absent from the file must not be reported as present.
{
  const truth = groundTruth('c', 'src/server.c', '^int serverCron\\(');
  const run = await locateAll({
    mainGoal: GOAL, reasoning: 'Absent target stays unresolved.',
    resources: [{ id: 'file', tool: 'localFetch', query: { path: truth.file } }],
    questions: [{ id: 't', type: 'locate', ask: 'The function that parses a YAML configuration file into nested dictionaries.' }],
  });
  const maxExists = Math.max(...run.pages.map(p => pageExists(p, 't')));
  check('absent target: every page exists < 0.5', maxExists < 0.5, `max exists=${maxExists} over ${run.pages.length} pages`);
}

// Matrix: two targets, one capture.
{
  const cron = groundTruth('c', 'src/server.c', '^int serverCron\\(');
  const proc = groundTruth('c', 'src/server.c', '^int processCommand\\(');
  const run = await locateAll({
    mainGoal: GOAL, reasoning: 'Two independent facts from one capture.',
    resources: [{ id: 'file', tool: 'localFetch', query: { path: cron.file } }],
    questions: [
      { id: 'cron', type: 'locate', ask: 'The periodic timer function that runs background housekeeping tasks many times per second.' },
      { id: 'proc', type: 'locate', ask: 'The function that validates a client command and decides whether to execute or queue it.' },
    ],
  });
  const bestFor = id => bestRows(run.final, id)[0];
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
      mainGoal: GOAL, reasoning: 'Choose the file to read among search hits.',
      resources: [{ id: 'hits', tool: 'localSearch', query: { path: root, matchString: 'func (sched *Scheduler)', regex: 'literal', resultView: 'files', pageSize: 20 } }],
      questions: [{ id: 'core', type: 'yesno', ask: 'Is this file the implementation of scheduling one pod (the main per-pod scheduling cycle)?' }],
    }] });
    const q = out.sc?.queries?.[0];
    const hits = q?.resources?.[0];
    const ranked = (hits?.pages ?? []).map(p => ({ path: p.path ?? p.source?.path ?? hits?.path ?? '', p: p.answers?.core?.yesno ?? p.answers?.core })).sort((a, b) => b.p - a.p);
    check('scout: schedule_one.go ranks first among search hits', ranked[0]?.path.endsWith('schedule_one.go'), ranked.slice(0, 3).map(r => `${path.basename(r.path)}=${r.p}`).join(', '));
  }
}

await scoutFlows();

// Judge: held snippets with known answers.
{
  const cases = [
    { value: 'func sum(n int) int { total := 0; for i := 0; i < n; i++ { total += i }; return total }', q: 'Does this code contain a loop?', yes: true },
    { value: 'def area(r):\n    return 3.14159 * r * r', q: 'Does this function perform any I/O (files, network, or printing)?', yes: false },
    { value: 'fn push(v: &mut Vec<i32>, x: i32) { v.push(x); }', q: 'Does this function mutate one of its arguments?', yes: true },
    { value: 'const add = (a, b) => a + b;', q: 'Does this code catch or handle exceptions?', yes: false },
  ];
  const out = await raw('clasify', { queries: [{
    mainGoal: GOAL, reasoning: 'Held-state calibration.',
    resources: cases.map((c, i) => ({ id: `c${i}`, value: c.value })),
    questions: cases.map((c, i) => ({ id: `q${i}`, type: 'yesno', ask: c.q })),
  }] });
  const resources = out.sc?.queries?.[0]?.resources ?? [];
  let correct = 0;
  cases.forEach((c, i) => {
    const p = answerOf(resources.find(r => r.id === `c${i}`), `q${i}`);
    if (typeof p === 'number' && (c.yes ? p >= 0.8 : p <= 0.2)) correct += 1;
  });
  check(`judge: ${correct}/${cases.length} held snippets answered decisively and correctly`, correct === cases.length, out.text.slice(0, 200));
}

// Errors: missing and binary files stay unresolved, never "no".
{
  const missing = await raw('clasify', { queries: [{ mainGoal: GOAL, reasoning: 'missing', resources: [{ tool: 'localFetch', query: { reasoning: 'x', path: path.join(REPOS, 'c/nope.c'), fullContent: true } }], questions: [{ id: 't', type: 'locate', ask: 'anything' }] }] });
  check('error: missing file is reported, not judged', /error|not found|File not found/i.test(missing.text) && !/"exists":0\./.test(missing.text), missing.text.slice(0, 160));
}

const result = summary();
writeResults('clasify', { table, prefilterQuality, ...result });
client.close();
process.exitCode = result.failed.length ? 1 : 0;
