// Classification against the real provider (live suite, paid calls).
//
// Retrieval `clasifyContext` selection (source order is kept; unselected and
// unjudged evidence is withheld): localFetch over large unread files must select
// the target declaration (ground truth from the source bytes, never from the
// model), localSearch must select the implementing file, localFetch over a huge
// file must select the target among its match windows, two classifiers over one
// file must both select their declarations, and a selected continuation walk
// must stay in source order while whole-view recovery returns every line once.
//
// Public `clasify`: typed noul/choice/score answers over held `state`, over a
// localFetch `source` (with an executable `hints.read`), over a URL capture,
// and an unreadable source failing its row instead of answering.
//
// Reports selected host-visible bytes and raw source-file bytes separately.
import fs from 'node:fs';
import path from 'node:path';
import { findHint, FIXTURES, REPOS, checks, rowData, sourceLines, startServer, writeResults } from './mcp-client.mjs';

const { check, skip, summary } = checks('clasify');
// Selection needs only the caller's provider key; beta exposes public clasify.
const client = await startServer({ env: { OCTOCODE_BETA: 'true' } });
const { raw, call, follow } = client;
const queryFields = name => client.tools.find(tool => tool.name === name)?.inputSchema?.properties?.queries?.items?.properties ?? {};
const noClassifiers = queryFields('localFetch').clasifyContext ? null : 'clasifyContext is not advertised (no classification provider key)';
const noClasify = client.tools.some(tool => tool.name === 'clasify') ? null : 'clasify is not registered (no provider key or beta off)';

/** The first line of `dir/file` matching `regex` (ground truth never comes from the model). */
function groundTruth(dir, file, regex) {
  const full = path.join(REPOS, dir, file);
  if (!fs.existsSync(full)) return null;
  const re = new RegExp(regex);
  const index = fs.readFileSync(full, 'utf8').split('\n').findIndex(line => re.test(line));
  return index < 0 ? null : { file: full, line: index + 1 };
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
// Classify the complete file; page size is controlled by executable continuations.
const FULL_FILE = { fullContent: true };
const MAX_SELECTED_PAGES = 20;

/** A selected view: the first page plus its plan-bound continueWindow pages (never next.unfiltered). */
async function selectedView(first) {
  const pages = [first];
  for (let lead = rowData(first)?.next?.continueWindow; lead && pages.length < MAX_SELECTED_PAGES; lead = rowData(pages.at(-1))?.next?.continueWindow) pages.push(await follow(lead));
  const lines = pages.flatMap(page => sourceLines(rowData(page)));
  return {
    pages, lines, bytes: pages.reduce((sum, page) => sum + page.bytes, 0),
    receipt: rowData(first)?.selection,
    complete: !rowData(pages.at(-1))?.next?.continueWindow,
    ok: pages.every(page => !page.isError && !page.rowErrors),
  };
}
/** The receipt summary a table row or failure detail shows. */
const receiptText = receipt => receipt ? `${receipt.status} ${receipt.selectedItems}/${receipt.totalItems} selected, ${receipt.unknownItems} unknown${receipt.reason ? ` (${receipt.reason})` : ''}` : '-';
/** Selected lines keep source order with no duplicate anchors. */
const sourceOrdered = lines => lines.every(([n], i) => i === 0 || n > lines[i - 1][0]);

const table = [];
if (noClassifiers) skip('classifiers: localFetch, localSearch and walk checks', noClassifiers);
else {
  for (const t of LOCATE) {
    const truth = groundTruth(t.dir, t.glob, t.regex);
    if (!check(`${t.name}: ground truth found`, truth, `${t.dir}/${t.glob}`)) continue;
    const size = fs.statSync(truth.file).size;
    const started = Date.now();
    const view = await selectedView(await call('localFetch', { path: truth.file, clasifyContext: { classifiers: [t.target] }, ...FULL_FILE }));
    const declRe = new RegExp(t.regex);
    // Strict: the selected view delivers the declaration line itself at its source anchor.
    const strict = view.lines.some(([n, text]) => n === truth.line && declRe.test(text));
    table.push({
      target: t.name, fileKB: Math.round(size / 1024), truth: truth.line, selection: receiptText(view.receipt),
      pages: view.pages.length, strict, selectedHostBytes: view.bytes, sourceFileBytes: size, s: ((Date.now() - started) / 1000).toFixed(0),
    });
    check(`${t.name}: the selected view delivers the declaration (line ${truth.line}) in source order`, view.ok && view.complete && strict && sourceOrdered(view.lines), `selection=${receiptText(view.receipt)} pages=${view.pages.length} first=${view.lines[0]?.[0]}`);
  }
  console.table(table);
  console.log(`clasifyContext selected declaration: ${table.filter(r => r.strict).length}/${table.length}`);

  // Search receipts count snippet groups; adjacent hit windows form one group.
  // A content view is required; file-list and count views reject clasifyContext.
  const scheduler = path.join(REPOS, 'huge-go/pkg/scheduler');
  if (!fs.existsSync(scheduler)) skip('clasifyContext localSearch: schedule_one.go is selected', `missing ${scheduler}`);
  else {
    const first = await call('localSearch', {
      path: scheduler, matchString: 'func (sched *Scheduler)', contextLines: 0,
      clasifyContext: { classifiers: ['The implementation of scheduling one pod: the main per-pod scheduling cycle.'] },
    });
    const pages = [];
    const pending = [first];
    const seen = new Set();
    let repeated = false;
    while (pending.length && pages.length < 100) {
      const page = pending.shift();
      pages.push(page);
      for (const key of ['nextPage', 'nextMatchPage']) {
        const lead = rowData(page)?.next?.[key];
        if (!lead) continue;
        const identity = JSON.stringify([lead.tool, lead.query]);
        if (seen.has(identity)) { repeated = true; continue; }
        seen.add(identity);
        pending.push(await follow(lead));
      }
    }
    const receipt = rowData(first)?.selection;
    const hits = pages.flatMap(page => (rowData(page)?.files ?? []).flatMap(file =>
      (file.matches ?? []).map(hit => ({ path: file.path, line: hit.line }))));
    const files = new Set(hits.map(hit => hit.path));
    const uniqueHits = new Set(hits.map(hit => JSON.stringify([hit.path, hit.line])));
    let groups = 0;
    for (const file of files) {
      const lines = [...new Set(hits.filter(hit => hit.path === file).map(hit => hit.line))].sort((a, b) => a - b);
      groups += lines.filter((line, index) => index === 0 || line > lines[index - 1] + 1).length;
    }
    check('clasifyContext localSearch: schedule_one.go is selected and other hits are withheld', pages.every(p => !p.isError && !p.rowErrors)
      && !pending.length && !repeated && uniqueHits.size === hits.length
      && [...files].some(file => file.endsWith('schedule_one.go')) && receipt?.unit === 'chunks'
      && receipt.selectedItems === groups && receipt.selectedItems < receipt.totalItems
      && receipt.selectedItems + receipt.excludedItems + (receipt.relatedItems ?? 0) + receipt.unknownItems === receipt.totalItems,
    JSON.stringify({ selection: receipt, groups, hits: hits.length, pages: pages.length, remaining: pending.length, files: [...files] }).slice(0, 400));
  }

  // Huge single file (3MB checker.ts): localFetch judges the match windows before delivery.
  {
    const truth = groundTruth('typescript', 'tsc/testdata/fixtures/compiler/checker.ts', 'function isTypeAssignableTo\\(');
    if (!truth) skip('huge file: selected match windows', 'missing checker.ts');
    else {
      const first = await call('localFetch', {
        path: truth.file, matchString: '(?i)assignable', regex: 'rust', contextLines: 2,
        clasifyContext: { classifiers: ['The function that checks whether a source type is assignable to a target type.'] },
      });
      const view = await selectedView(first);
      check(`huge file (3MB checker.ts): the selected match windows hold line ${truth.line}`, view.ok && view.lines.some(([n]) => n === truth.line) && sourceOrdered(view.lines),
        JSON.stringify({ selection: view.receipt, pages: view.pages.length, lines: view.lines.slice(0, 10).map(([n]) => n) }));
      console.log(JSON.stringify({ target: 'huge file match windows', selectedHostBytes: view.bytes, sourceFileBytes: fs.statSync(truth.file).size, pages: view.pages.length }));
    }
  }

  // Two classifiers over one file: either classifier's direct evidence is selected.
  {
    const cron = groundTruth('c', 'src/server.c', '^int serverCron\\(');
    const proc = groundTruth('c', 'src/server.c', '^int processCommand\\(');
    if (!cron || !proc) skip('two classifiers: both declarations', 'missing redis server.c');
    else {
      const view = await selectedView(await call('localFetch', {
        path: cron.file, ...FULL_FILE,
        clasifyContext: {
          classifiers: [
            'The periodic timer function that runs background housekeeping tasks many times per second.',
            'The function that validates a client command and decides whether to execute or queue it.',
          ],
        },
      }));
      const delivered = new Set(view.lines.map(([n]) => n));
      check('two classifiers: both declarations are in the selected view', view.ok && delivered.has(cron.line) && delivered.has(proc.line) && sourceOrdered(view.lines), JSON.stringify({ selection: view.receipt, pages: view.pages.length, cron: cron.line, proc: proc.line }));
    }
  }

  // Selected replay: the plan-bound walk keeps source order, holds the deciding
  // line, withholds the filler, and terminates; whole-view recovery reaches every line once.
  {
    // Under the workspace (the server's allowed root): the OS temp dir is outside it.
    const scratch = path.join(FIXTURES, 'scratch');
    fs.mkdirSync(scratch, { recursive: true });
    const dir = fs.mkdtempSync(path.join(scratch, 'clasify-walk-'));
    const file = path.join(dir, 'retry-guide.md');
    try {
      const lines = Array.from({ length: 429 }, (_, i) => `Line ${i + 1}: operational documentation filler explaining ordinary housekeeping and source ownership.`);
      lines[172] = 'Retry only after a transient upstream timeout; stop retrying after three attempts.';
      fs.writeFileSync(file, lines.join('\n') + '\n');
      const first = await call('localFetch', { path: file, clasifyContext: { classifiers: ['When must a retry stop?'] }, ...FULL_FILE });
      const view = await selectedView(first);
      const r = view.receipt;
      check('walk: every selected page succeeds', view.ok, view.pages.filter(p => p.isError || p.rowErrors).map(p => p.text.slice(0, 80)).join(' | '));
      check('walk: the selected view holds the deciding line', view.lines.some(([n]) => n === 173), receiptText(r));
      check('walk: selected pages keep source order, carry one receipt, terminate, and withhold the filler', view.complete && sourceOrdered(view.lines)
        && r?.selectedItems + r?.excludedItems + (r?.relatedItems ?? 0) + r?.unknownItems === r?.totalItems && r.selectedItems < r.totalItems && view.lines.length < 429
        && view.pages.every(page => JSON.stringify(rowData(page)?.selection) === JSON.stringify(r)), `${view.pages.length} pages, ${view.lines.length} lines, ${receiptText(r)}`);
      const next = rowData(first)?.next;
      const unfiltered = next?.unfiltered;
      const recovered = unfiltered ? [await follow(unfiltered)] : [];
      for (let lead = recovered[0] && rowData(recovered[0])?.next?.continueWindow; lead && recovered.length < 20; lead = rowData(recovered.at(-1))?.next?.continueWindow) recovered.push(await follow(lead));
      const seen = recovered.flatMap(p => sourceLines(rowData(p)).map(([n]) => n));
      const once = seen.length === 429 && new Set(seen).size === 429 && seen.every(n => n >= 1 && n <= 429);
      check('walk: whole unfiltered recovery drops the plan and recovers lines 1-429 exactly once', !!unfiltered && unfiltered.query?.queries?.[0]?.clasifyContext === undefined
        && recovered.every(p => !p.isError && !p.rowErrors && rowData(p)?.selection === undefined) && once && !rowData(recovered.at(-1))?.next?.continueWindow,
      `${recovered.length} pages, ${seen.length} lines, ${new Set(seen).size} distinct`);
    } finally {
      fs.rmSync(dir, { recursive: true, force: true });
    }
  }
}

/** One clasify row: its answers, or its error. */
const clasifyRow = out => out.sc?.queries?.[0] ?? {};
if (noClasify) skip('clasify: state, source, URL and error checks', noClasify);
else {
  // Held state: one row per snippet, each a noul with a known answer.
  {
    const cases = [
      { value: 'func sum(n int) int { total := 0; for i := 0; i < n; i++ { total += i }; return total }', q: 'Does this code contain a loop?', yes: true },
      { value: 'def area(r):\n    return 3.14159 * r * r', q: 'Does this function perform any I/O (files, network, or printing)?', yes: false },
      { value: 'fn push(v: &mut Vec<i32>, x: i32) { v.push(x); }', q: 'Does this function mutate one of its arguments?', yes: true },
      { value: 'const add = (a, b) => a + b;', q: 'Does this code catch or handle exceptions?', yes: false },
    ];
    const out = await raw('clasify', { queries: cases.map((c, i) => ({ id: `c${i}`, state: c.value, questions: { q: { type: 'noul', instructions: c.q } } })) });
    const rows = out.sc?.queries ?? [];
    const correct = cases.filter((c, i) => {
      const p = rows.find(r => r.id === `c${i}`)?.answers?.q?.noul;
      return typeof p === 'number' && (c.yes ? p >= 0.8 : p <= 0.2);
    }).length;
    check(`state: ${correct}/${cases.length} held snippets answered decisively and correctly`, correct === cases.length, out.text.slice(0, 200));
    // A literal state carries model and usage, and no source-read lead.
    check('state: rows carry model and usage, and no hints.read', rows.length === cases.length && rows.every(r => typeof r.model === 'string' && typeof r.usage?.input_tokens === 'number' && !r.hints), JSON.stringify(rows[0]).slice(0, 200));
  }

  // All three primitives over one state, provider fields preserved.
  {
    const out = await raw('clasify', { queries: [{ id: 'p', state: 'The fixture explicitly states that alpha is enabled.', questions: {
      stated: { type: 'noul', instructions: 'Does the fixture state that alpha is enabled?' },
      state: { type: 'choice', instructions: 'Which state does the fixture assign to alpha?', criteria: { enabled: 'alpha is enabled', disabled: 'alpha is disabled', unclear: 'the fixture does not say' } },
      explicit: { type: 'score', instructions: 'How explicit is the fixture about alpha being enabled?', criteria: ['not stated', 'implied', 'explicitly stated'] },
    } }] });
    const a = clasifyRow(out).answers ?? {};
    check('primitives: noul, choice (with probabilities) and score (with legend) answer as typed', a.stated?.type === 'noul' && a.stated.noul >= 0.8
      && a.state?.type === 'choice' && a.state.choice === 'enabled' && typeof a.state.probabilities?.enabled === 'number'
      && a.explicit?.type === 'score' && a.explicit.score >= 1.5 && a.explicit.legend?.['2'] === 'explicitly stated', JSON.stringify(a).slice(0, 220));
  }

  // localFetch source: positive and negative questions over one captured range,
  // no body returned, and the read lead returns exactly the captured lines.
  {
    const cron = groundTruth('c', 'src/server.c', '^int serverCron\\(');
    if (!cron) skip('source: localFetch range', 'missing redis server.c');
    else {
      const range = `${cron.line}-${cron.line + 60}`;
      const out = await raw('clasify', { queries: [{ id: 'cron', source: { tool: 'localFetch', query: { path: cron.file, ranges: [range] } }, questions: {
        timer: { type: 'noul', instructions: 'Does the selection define the periodic server timer function serverCron?' },
        yaml: { type: 'noul', instructions: 'Does the selection parse a YAML configuration file into nested dictionaries?' },
      } }] });
      const row = clasifyRow(out);
      check('source: a present target answers yes and an absent one no', row.answers?.timer?.noul >= 0.5 && row.answers?.yaml?.noul < 0.5, JSON.stringify(row.answers ?? row.error));
      check('source: no source body is returned', !out.text.includes('serverCron(struct aeEventLoop'), out.text.slice(0, 160));
      const lead = findHint(row, 'read');
      const read = lead && await follow(lead);
      const lines = sourceLines(read && rowData(read));
      check('source: hints.read is a pinned localFetch of the captured range', lead?.tool === 'localFetch' && typeof lead.query?.queries?.[0]?.snapshot === 'string'
        && lines[0]?.[0] === cron.line && lines.at(-1)?.[0] === cron.line + 60, JSON.stringify(lead).slice(0, 200));
    }
  }

  // URL source: captured, judged, and readable back through the saved capture.
  {
    const out = await raw('clasify', { queries: [{ id: 'url', source: { url: 'https://www.rfc-editor.org/rfc/rfc2606.txt' }, questions: {
      reserved: { type: 'noul', instructions: 'Does the document reserve example.com for documentation use?' },
    } }] });
    const row = clasifyRow(out);
    check('url: the page is judged and its provenance carries snapshot and expiresAt', row.answers?.reserved?.noul >= 0.5 && typeof row.source?.snapshot === 'string' && typeof row.source?.expiresAt === 'string', JSON.stringify(row).slice(0, 220));
    const lead = findHint(row, 'read');
    const read = lead && await follow(lead);
    check('url: hints.read returns the captured text through localFetch', !!read && !read.isError && /example\.com/.test(rowData(read)?.content ?? ''), read?.text.slice(0, 160) ?? 'no hints.read');
  }

  // Errors: an unreadable source fails its row, never answers "no".
  {
    const out = await raw('clasify', { queries: [{ id: 'missing', source: { tool: 'localFetch', query: { path: path.join(REPOS, 'c/nope.c'), fullContent: true } }, questions: { t: { type: 'noul', instructions: 'Anything?' } } }] });
    const row = clasifyRow(out);
    check('error: a missing source fails its row with an errorCode and no answers', typeof row.error?.errorCode === 'string' && !row.answers, JSON.stringify(row).slice(0, 200));
  }
}

const result = summary();
writeResults('clasify', { table, ...result });
client.close();
process.exitCode = result.failed.length ? 1 : 0;
