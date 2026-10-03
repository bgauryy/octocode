// Agent-style navigation: follow the server instructions' loop
// (ORIENT → SEARCH → READ EXACT → PROVE) in every grammar repo, feeding each
// hop only values copied from the previous result (path via base+path,
// symbolName/lineHint from symbols rows, anchors from LSP items). Measures
// hops and bytes, and fails when a hop's promised handoff is missing.
import fs from 'node:fs';
import path from 'node:path';
import { REPOS, checks, collect, declarations, rowData, sourcePath, startServer, structureFiles, writeResults, lspLocations } from './mcp-client.mjs';

const { check, summary } = checks('navigate');
const client = await startServer();
const { call } = client;

const TARGETS = [
  { lang: 'TypeScript (3MB checker.ts)', dir: 'typescript', scope: 'tsc/testdata/fixtures/compiler', ext: ['ts'], lsp: true, rg: 'ts' },
  { lang: 'TSX', dir: 'tsx', scope: 'packages/excalidraw/components', ext: ['tsx'], lsp: true, rg: 'ts' },
  { lang: 'JavaScript', dir: 'javascript', scope: '.', ext: ['js'], lsp: true, rg: 'js' },
  { lang: 'Python', dir: 'python', scope: 'django/db/models', ext: ['py'], lsp: false, rg: 'py' },
  { lang: 'Go', dir: 'go', scope: 'tsdb', ext: ['go'], lsp: false, rg: 'go' },
  { lang: 'Rust', dir: 'rust', scope: 'tokio/src/sync', ext: ['rs'], lsp: true, rg: 'rust' },
  { lang: 'Java', dir: 'java', scope: 'guava/src/com/google/common/base', ext: ['java'], lsp: false, rg: 'java' },
  { lang: 'C', dir: 'c', scope: 'src', ext: ['c'], lsp: true, rg: 'c' },
  { lang: 'C++', dir: 'cpp', scope: 'include/nlohmann/detail/input', ext: ['hpp'], lsp: true, rg: 'cpp', langType: 'cpp' },
  { lang: 'C#', dir: 'csharp', scope: 'Src/Newtonsoft.Json/Linq', ext: ['cs'], lsp: false, rg: 'csharp' },
  { lang: 'Scala', dir: 'scala', scope: 'core/src/main/scala/cats/data', ext: ['scala'], lsp: false, rg: 'scala' },
  { lang: 'Assembly', dir: 'asm', scope: 'simd/x86_64', ext: ['asm'], lsp: false, rg: 'asm' },
];

const HUGE = [
  { lang: 'C (linux, 96k files)', dir: 'huge-c', scope: 'kernel/sched', ext: ['c'], lsp: true, rg: 'c' },
  { lang: 'Go (kubernetes, 31k files)', dir: 'huge-go', scope: 'pkg/scheduler', ext: ['go'], lsp: false, rg: 'go' },
  { lang: 'TypeScript (vscode, 19k files)', dir: 'huge-ts', scope: 'src/vs/editor/common/model', ext: ['ts'], lsp: true, rg: 'ts' },
  { lang: 'Rust (rustc, 63k files)', dir: 'huge-rust', scope: 'compiler/rustc_borrowck/src', ext: ['rs'], lsp: false, rg: 'rust' },
  { lang: 'Java (elasticsearch, 49k files)', dir: 'huge-java', scope: 'server/src/main/java/org/elasticsearch/index/engine', ext: ['java'], lsp: false, rg: 'java' },
  { lang: 'C++ (pytorch, 22k files)', dir: 'huge-cpp', scope: 'torch/csrc/autograd', ext: ['cpp'], lsp: true, rg: 'cpp' },
];
if (process.argv[2] === 'huge') TARGETS.splice(0, TARGETS.length, ...HUGE);
const abs = (entry, p) => path.resolve(entry.sc?.base ?? '/', p);
const table = [];
for (const t of TARGETS) {
  const hops = [];
  const hop = async (label, tool, query) => {
    const e = await call(tool, query);
    hops.push({ label, tool, bytes: e.bytes, ms: e.ms, error: e.isError || e.rowErrors > 0 });
    return e;
  };
  const root = path.join(REPOS, t.dir);
  const row = { lang: t.lang };

  // ORIENT: layout, then the biggest source file in scope.
  const tree = await hop('orient: tree', 'structureSearch', { operation: 'tree', path: path.join(root, t.scope), maxDepth: 1 });
  const listing = await hop('orient: files', 'structureSearch', { operation: 'files', path: path.join(root, t.scope), extensions: t.ext, detail: 'full', sort: 'lines', limit: 1 });
  const top = structureFiles(rowData(listing)?.files).filter(o => typeof o.path === 'string' && typeof o.lineCount === 'number')[0];
  if (!check(`${t.lang}: orient yields a file`, !tree.isError && top, listing.text.slice(0, 80))) { table.push(row); continue; }
  const file = abs(listing, top.path);
  check(`${t.lang}: base + path is a real file`, fs.existsSync(file), file);
  row.file = `${path.basename(file)} (${top.lineCount} lines)`;

  // SEARCH (shape): outline declarations; take a callable used elsewhere.
  const symbols = await hop('search: symbols', 'astSearch', { operation: 'symbols', path: file, kinds: ['function', 'method', 'label'], pageSize: 100, ...(t.langType ? { langType: t.langType } : {}) });
  const decls = declarations(symbols).filter(o => typeof o.name === 'string' && typeof o.line === 'number' && typeof o.kind === 'string');
  const callables = decls.filter(d => ['function', 'method', 'label'].includes(d.kind) && d.name.length > 4);
  let target;
  let uses = [];
  for (const candidate of callables.slice(0, 25)) {
    const s = await call('localSearch', { path: root, searchText: candidate.name, wholeWord: true, langType: t.rg, maxMatchesPerFile: 3, pageSize: 20 });
    const hits = (rowData(s)?.files ?? []).flatMap(f => (f.matches ?? []).map(m => ({ file: abs(s, f.path), line: m.line })));
    if (hits.some(h => h.file !== file || h.line !== candidate.line)) { target = candidate; uses = hits; hops.push({ label: 'search: text uses', tool: 'localSearch', bytes: s.bytes, ms: s.ms }); break; }
  }
  if (!check(`${t.lang}: a declaration with a use elsewhere`, target, `callables=${callables.length}`)) { table.push(row); continue; }
  row.symbol = `${target.name}@${target.line}`;

  // READ EXACT: the declaration lines, anchored on the symbols row.
  const read = await hop('read: declaration', 'localFetch', { path: file, startLine: target.line, endLine: Math.min(target.line + 25, top.lineCount) });
  check(`${t.lang}: read-exact shows the declaration`, (rowData(read)?.content ?? '').includes(target.name), rowData(read)?.errorCode ?? '');

  // PROVE: identity via LSP from the copied anchor, else text + syntax.
  if (t.lsp) {
    const def = await hop('prove: definition', 'lspSearch', { uri: file, symbolName: target.name, lineHint: target.line, operation: 'definition' });
    const lands = (rowData(def)?.payload?.locations ?? []).some(l => sourcePath(def, l, file) === file && Math.abs((l.displayRange?.startLine ?? 0) - target.line) <= 1);
    // The server may resolve a declaration name to a merged declaration
    // elsewhere (TS: a function merged with an imported interface); accept
    // any landing that astSearch confirms declares the same name.
    let confirmed = false;
    for (const l of rowData(def)?.payload?.locations ?? []) {
      const at = sourcePath(def, l, file);
      const d = await call('astSearch', { operation: 'symbols', path: at, name: target.name, ...(t.langType ? { langType: t.langType } : {}) });
      if (declarations(d).filter(o => o.name === target.name && Math.abs(o.line - (l.displayRange?.startLine ?? 0)) <= 1).length) confirmed = true;
    }
    const explained = /compile_commands/.test(JSON.stringify(rowData(def)?.hints ?? []));
    check(`${t.lang}: symbols anchor → lspSearch definition lands on an astSearch-confirmed declaration`, lands || confirmed || explained, JSON.stringify(rowData(def)?.payload?.locations?.[0]?.displayRange ?? rowData(def)?.hints ?? ''));
    const refs = await hop('prove: references', 'lspSearch', { uri: file, symbolName: target.name, lineHint: target.line, operation: 'references', pageSize: 25 });
    const refFiles = new Set(lspLocations(refs).map(l => sourcePath(refs, l, file)));
    // Every text-hit file (all pages), not just the sampled uses.
    const textFiles = new Set(uses.map(u => u.file));
    let page = await call('localSearch', { path: root, searchText: target.name, wholeWord: true, langType: t.rg, resultView: 'files', pageSize: 100 });
    for (let i = 0; page && i < 30; i++) {
      for (const f of rowData(page)?.files ?? []) textFiles.add(abs(page, f.path));
      const next = rowData(page)?.next?.nextPage;
      page = next ? await client.raw(next.tool, next.query) : null;
    }
    const outside = [...refFiles].filter(f => !textFiles.has(f));
    row.refs = `${refFiles.size} files`;
    check(`${t.lang}: LSP reference files ⊆ text-hit files`, refs.isError ? false : outside.length === 0, outside.slice(0, 2).join(','));
  } else {
    const use = uses.find(h => h.file !== file || h.line !== target.line);
    const proof = await hop('prove: read use site', 'localFetch', { path: use.file, startLine: use.line, endLine: use.line });
    check(`${t.lang}: text anchor → localFetch line names the symbol`, (rowData(proof)?.content ?? '').includes(target.name));
  }
  const total = hops.reduce((sum, h) => sum + h.bytes, 0);
  row.hops = hops.length;
  row.bytes = total;
  row.maxHop = Math.max(...hops.map(h => h.bytes));
  check(`${t.lang}: every hop succeeded`, hops.every(h => !h.error), hops.filter(h => h.error).map(h => h.label).join(','));
  check(`${t.lang}: navigation stays lean (≤80KB total)`, total <= 80_000, `${total}B over ${hops.length} hops`);
  table.push(row);
}
console.table(table);
const result = summary();
writeResults(process.argv[2] === 'huge' ? 'navigate-huge' : 'navigate', { table, ...result });
client.close();
process.exitCode = result.failed.length ? 1 : 0;
