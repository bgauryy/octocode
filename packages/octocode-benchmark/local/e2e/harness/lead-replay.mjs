// Lead replay: run every follow-up lead a fixture call emits and
// check it does what its key promises. A lead is `{tool, query}` under `next.*`
// or `hints.*`; the recurring defect is a key that says one thing while its
// query does another (an includeHidden lead that only lifts defaultExcludes, a
// findText lead whose regex ran as literal text, a narrowScope lead with an
// unpinned ref that returns notFound, a viewReleaseSource lead on a build path).
//
// Phase 1 runs a fixture set over the local corpus (a synthetic workspace with
// ignored/hidden/excluded/paged content, repos/*, fixtures/large) and, with
// OCTOCODE_LIVE=1 (the live part), a few GitHub and registry calls. Phase 2 executes every distinct
// lead verbatim (bounded per key, two levels deep). Each lead is checked by
// predicates chosen from its KEY FAMILY (harness/lead-predicates.mjs lists them),
// never per fixture.
// A failed assertion FAILs with source, key, query, outcome.
//
//   node harness/lead-replay.mjs      (OCTOCODE_LEAD_REPLAY_BUDGET_MS, default 140000)
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { FIXTURES, LIVE, ROOT, checks, cliEntry, startServer, writeResults } from './mcp-client.mjs';
import { PAGE_KEYS, atoms, canon, judgeLead, leadsOf, outcomeOf, properties, rowOf, scopeChanges, short, sourceScopeForPage } from './lead-predicates.mjs';
import { createQueryDefaults } from './query-defaults.mjs';

const SUITE = 'lead-replay';
const defaultsOf = createQueryDefaults(JSON.parse(fs.readFileSync(path.join(ROOT, 'packages/octocode-config/contract/tool-contract.json'), 'utf8')).tools);
const { check, skip, summary } = checks(SUITE);
const BUDGET_MS = Number(process.env.OCTOCODE_LEAD_REPLAY_BUDGET_MS ?? 140_000);
const STARTED = Date.now();
const PER_KEY = Number(process.env.OCTOCODE_LEAD_REPLAY_PER_KEY ?? 4);
const CONCURRENCY = 4;
const MAX_DEPTH = 2;
const WALK_PAGES = 2;

const SKIP_EXEC_KEYS = { cloneRepo: 'writes the clone cache', exploreClone: 'writes the clone cache', clasify: 'paid provider call' };

// Fixture workspace: own git repo so its .gitignore applies; ignored, hidden, default-excluded, paged and binary content.
const WREL = `.octocode/tmp/lead-replay-${process.pid}`;
const WABS = path.join(ROOT, WREL);
const SREL = `${WREL}-stale`; // its own corpus: mutating it must not stale any snapshot taken from the main workspace
const SABS = path.join(ROOT, SREL);
const PREL = `${WREL}-count-pages`;
const PABS = path.join(ROOT, PREL);
const write = (rel, text, base = WABS) => { const f = path.join(base, rel); fs.mkdirSync(path.dirname(f), { recursive: true }); fs.writeFileSync(f, text); };
function buildWorkspace() {
  fs.rmSync(WABS, { recursive: true, force: true });
  fs.rmSync(SABS, { recursive: true, force: true });
  fs.rmSync(PABS, { recursive: true, force: true });
  fs.mkdirSync(WABS, { recursive: true });
  fs.mkdirSync(SABS, { recursive: true });
  fs.mkdirSync(PABS, { recursive: true });
  for (const dir of [WABS, SABS, PABS]) spawnSync('git', ['init', '-q'], { cwd: dir });
  write('.gitignore', 'build/\n*.log\n');
  write('package.json', JSON.stringify({ name: 'lead-replay-fixture', private: true, main: 'src/a.ts' }));
  write('tsconfig.json', JSON.stringify({ compilerOptions: { target: 'es2022', module: 'esnext', strict: true } }));
  write('src/a.ts', "import { helper } from './b';\nexport function visibleSym(n: number): number {\n  return helper(n) + 1;\n}\nexport const needleToken = 'needleToken';\n");
  write('src/b.ts', "import { visibleSym } from './a';\nexport function helper(n: number): number {\n  return n > 100 ? visibleSym(n - 1) : n * 2;\n}\n");
  write('src/many.ts', Array.from({ length: 90 }, (_, i) => `export const needleToken_${i} = ${i}; // needleToken ${i}`).join('\n') + '\n');
  for (let i = 0; i < 25; i++) write(`src/gen/f${String(i).padStart(2, '0')}.ts`, `export const gen${i} = ${i}; // needleToken in gen ${i}\n`);
  for (let i = 0; i < 8; i++) write(`s${String(i).padStart(2, '0')}.ts`, `export const s${i} = ${i}; // needleStale ${i}\n`, SABS);
  write('src/shapes.ts', 'export interface Shape { area(): number }\nexport class Circle implements Shape { area(): number { return 3; } }\nexport class Square implements Shape { area(): number { return 4; } }\nexport class Rounded extends Circle {}\n');
  write('build/out.js', 'export function ignoredSym() { return 1; }\n');
  write('app.log', 'ignoredSym in a log\n');
  write('.hidden/h.ts', 'export function hiddenSym() { return 1; }\n');
  write('node_modules/pkg/i.js', 'export function defaultExcludedSym() { return 1; }\n');
  write('big.txt', Array.from({ length: 4000 }, (_, i) => `line ${i + 1} lorem ipsum dolor sit amet needleBig${i % 500 === 0 ? 'Hit' : ''}`).join('\n') + '\n');
  write('minified.js', `${Array.from({ length: 3500 }, (_, i) => `function f${i}(a){return a+${i}}`).join(';')};var needleToken=1;\n`);
  fs.writeFileSync(path.join(WABS, 'blob.bin'), Buffer.from(Array.from({ length: 2048 }, (_, i) => (i * 7) % 256)));
  for (let i = 0; i < 101; i++) write(`f${String(i).padStart(3, '0')}.ts`, `export const page${i} = 'needleCountPages';\n`, PABS);
}

// Fixture calls: [tool, query, label]. Empty, scope-miss, gitignored, paging and failure cases per tool.
const W = WREL;
const RS = 'packages/octocode-benchmark/local/repos/rust/tokio';
const H = ['hints'];
const fixtures = () => [
  // localSearch
  ['localSearch', { path: RS, matchString: 'fn spawn', pageSize: 3, extras: H, mainGoal: 'find spawn', reasoning: 'inheritance probe' }, 'ls tokio paged + mainGoal'],
  ['localSearch', { path: `${W}/src`, matchString: 'needleToken', pageSize: 5, extras: H }, 'ls fixture paged'],
  ['localSearch', { path: SREL, matchString: 'needleStale', pageSize: 3, extras: H }, 'ls stale source'],
  ['localSearch', { path: `${W}/src/gen`, matchString: 'needleToken', pageSize: 4, resultView: 'files', extras: H }, 'ls files view paged'],
  ['localSearch', { path: `${RS}/src`, matchString: 'fn ', extras: H }, 'ls tokio default size'],
  ['localSearch', { path: `${RS}/src/runtime`, matchString: 'fn (spawn|block_on)\\b', regex: 'rust', pageSize: 4, extras: H }, 'ls regex rust'],
  ['localSearch', { path: `${RS}/src`, matchString: 'spawn(|block_on', regex: 'rust', extras: H }, 'ls regex broken (repair)'],
  ['localSearch', { path: `${W}/src`, matchString: 'NEEDLETOKEN', extras: H }, 'ls case mismatch'],
  ['localSearch', { path: `${W}/src`, matchString: 'needleToken_(?=1\\d)\\w+', regex: 'pcre2', pageSize: 5, extras: H }, 'ls regex pcre2'],
  ['localSearch', { path: `${W}/src`, matchString: 'needleToken|visibleSym', pageSize: 5, extras: H }, 'ls literal pipe'],
  ['localSearch', { path: W, matchString: 'zzqqNeverMatches', extras: H }, 'ls empty'],
  ['localSearch', { path: `${W}/nope`, matchString: 'x', extras: H }, 'ls scope miss'],
  ['localSearch', { path: W, matchString: 'ignoredSym', extras: H }, 'ls gitignored only'],
  ['localSearch', { path: W, matchString: 'hiddenSym', extras: H }, 'ls hidden only'],
  ['localSearch', { path: W, matchString: 'defaultExcludedSym', extras: H }, 'ls default-excluded only'],
  ['localSearch', { path: W, matchString: 'Sym', extras: H }, 'ls visible + skipped'],
  ['localSearch', { path: `${W}/src`, matchString: 'needleToken', resultView: 'filesWithout', extras: H }, 'ls filesWithout'],
  ['localSearch', { path: `${W}/src`, matchString: 'needleToken', resultView: 'countLines', extras: H }, 'ls countLines'],
  ['localSearch', { path: `${W}/src`, matchString: 'needleToken', resultView: 'countMatches', extras: H }, 'ls countMatches'],
  ['localSearch', { path: `${W}/src`, matchString: 'needleToken', resultView: 'matchOnly', pageSize: 5, extras: H }, 'ls matchOnly'],
  ['localSearch', { path: `${W}/src`, matchString: 'needleToken', fileHas: ['visibleSym'], fileLacks: ['zzqq'], extras: H }, 'ls fileHas/fileLacks'],
  ['localSearch', { path: `${W}/src`, matchString: 'needleToken', include: ['*.ts'], exclude: ['gen'], contextLines: 2, pageSize: 3, extras: H }, 'ls include/exclude/context'],
  ['localSearch', { path: `${W}/minified.js`, matchString: 'needleToken', extras: H }, 'ls minified clipped line'],
  ['localSearch', { path: 'packages/octocode-benchmark/local/repos/javascript', matchString: 'function', pageSize: 3, extras: H }, 'ls javascript corpus'],
  ['localSearch', { path: 'packages/octocode-benchmark/local/repos/python', matchString: 'def ', pageSize: 3, extras: H }, 'ls python corpus'],
  // localFetch
  ['localFetch', { path: `${W}/big.txt`, extras: H }, 'lf large default window'],
  ['localFetch', { path: `${W}/big.txt`, fullContent: true, extras: H }, 'lf large fullContent'],
  ['localFetch', { path: `${W}/big.txt`, matchString: 'needleBigHit', contextLines: 1, extras: H }, 'lf matchString windows'],
  ['localFetch', { path: `${W}/big.txt`, ranges: ['10-20', '300-305'], extras: H }, 'lf ranges'],
  ['localFetch', { path: `${W}/big.txt`, matchString: 'needleBig(Hit)?', regex: 'rust', extras: H }, 'lf regex rust'],
  ['localFetch', { path: `${RS}/src/task/join_set.rs`, matchString: 'pub fn spawn<F>', block: true, extras: H }, 'lf block'],
  ['localFetch', { path: `${RS}/src/task/join_set.rs`, minify: 'symbols', extras: H }, 'lf outline'],
  ['localFetch', { path: `${W}/minified.js`, extras: H }, 'lf minified one line'],
  ['localFetch', { path: `${W}/minified.js`, matchString: 'needleToken', extras: H }, 'lf minified match'],
  ['localFetch', { path: `${W}/big.txt`, matchString: 'zzqqNeverMatches', extras: H }, 'lf empty'],
  ['localFetch', { path: `${W}/big.txt`, matchString: 'NeedleBighit', extras: H }, 'lf smart-case miss'],
  ['localFetch', { path: `${W}/nope.txt`, extras: H }, 'lf scope miss'],
  ['localFetch', { path: `${W}/blob.bin`, extras: H }, 'lf binary'],
  ['localFetch', { path: path.join(FIXTURES, 'large/huge.log'), extras: H }, 'lf huge log'],
  ['localFetch', { path: `${W}/build/out.js`, matchString: 'ignoredSym', extras: H }, 'lf gitignored file'],
  // structureSearch
  ['structureSearch', { path: RS, extras: H }, 'ss tree'],
  ['structureSearch', { path: W, maxDepth: 2, extras: H }, 'ss tree fixture'],
  ['structureSearch', { path: `${W}/src`, operation: 'tree', pageSize: 5, extras: H }, 'ss tree paged'],
  ['structureSearch', { path: 'packages/octocode-benchmark/local/repos/python', operation: 'files', include: ['*.py'], extras: H }, 'ss files python size page'],
  ['structureSearch', { path: RS, operation: 'files', extensions: ['rs'], maxEntries: 40, extras: H }, 'ss files maxEntries ceiling'],
  ['structureSearch', { path: `${W}/src`, operation: 'files', extensions: ['ts'], pageSize: 6, extras: H }, 'ss files paged'],
  ['structureSearch', { path: W, operation: 'files', include: ['out.js'], extras: H }, 'ss files gitignored only'],
  ['structureSearch', { path: W, operation: 'files', include: ['h.ts'], extras: H }, 'ss files hidden only'],
  ['structureSearch', { path: W, operation: 'files', include: ['i.js'], extras: H }, 'ss files default-excluded only'],
  ['structureSearch', { path: W, operation: 'files', include: ['*.zzqq'], extras: H }, 'ss files empty'],
  ['structureSearch', { path: `${W}/nope`, extras: H }, 'ss scope miss'],
  ['structureSearch', { path: 'packages/octocode-benchmark/local/repos/typescript', operation: 'files', include: ['**/*.ts'], extras: H }, 'ss files huge corpus'],
  // astSearch
  ['astSearch', { path: `${RS}/src/task/join_set.rs`, operation: 'symbols', extras: H }, 'as symbols file'],
  ['astSearch', { path: `${RS}/src/task`, operation: 'symbols', kinds: ['function'], extras: H }, 'as symbols dir'],
  ['astSearch', { path: `${W}/src`, operation: 'symbols', symbolName: 'gen', extras: H }, 'as symbols by name'],
  ['astSearch', { path: `${RS}/src/task`, operation: 'match', pattern: 'pub fn $NAME($$$ARGS) $$$RET { $$$BODY }', language: 'rust', extras: H }, 'as match'],
  ['astSearch', { path: `${W}/src`, operation: 'match', pattern: 'export const $N = $V', language: 'typescript', extras: H }, 'as match fixture'],
  ['astSearch', { path: `${W}/src`, operation: 'match', rule: { kind: 'function_declaration', has: { pattern: 'helper($A)', stopBy: 'end' } }, language: 'typescript', extras: H }, 'as rule'],
  ['astSearch', { path: `${W}/src/a.ts`, operation: 'syntaxTree', extras: H }, 'as syntaxTree'],
  ['astSearch', { path: `${W}/src`, operation: 'symbols', symbolName: 'zzqqNeverMatches', extras: H }, 'as empty'],
  ['astSearch', { path: W, operation: 'symbols', symbolName: 'ignoredSym', extras: H }, 'as gitignored only'],
  ['astSearch', { path: `${W}/src`, operation: 'match', pattern: 'function $F(', language: 'typescript', extras: H }, 'as invalid pattern'],
  ['astSearch', { path: `${W}/nope`, operation: 'symbols', extras: H }, 'as scope miss'],
  ['astSearch', { path: path.join(FIXTURES, 'large/many-functions.ts'), operation: 'symbols', symbolName: 'helper', extras: H }, 'as too-large file'],
  // lspSearch
  ['lspSearch', { path: `${W}/src/a.ts`, operation: 'documentSymbols', extras: H }, 'lsp documentSymbols'],
  ['lspSearch', { path: `${W}/src/b.ts`, operation: 'definition', symbolName: 'visibleSym', line: 3, extras: H }, 'lsp definition'],
  ['lspSearch', { path: `${W}/src/a.ts`, operation: 'references', symbolName: 'visibleSym', line: 2, extras: H }, 'lsp references'],
  ['lspSearch', { path: `${W}/src/a.ts`, operation: 'callers', symbolName: 'visibleSym', line: 2, extras: H }, 'lsp callers'],
  ['lspSearch', { path: `${W}/src/a.ts`, operation: 'callees', symbolName: 'visibleSym', line: 2, extras: H }, 'lsp callees'],
  ['lspSearch', { path: `${W}/src/a.ts`, operation: 'hover', symbolName: 'visibleSym', line: 2, extras: H }, 'lsp hover'],
  ['lspSearch', { path: `${W}/src/shapes.ts`, operation: 'typeDefinition', symbolName: 'Shape', line: 2, extras: H }, 'lsp typeDefinition'],
  ['lspSearch', { path: `${W}/src/shapes.ts`, operation: 'implementation', symbolName: 'Shape', line: 1, extras: H }, 'lsp implementation'],
  ['lspSearch', { path: `${W}/src/shapes.ts`, operation: 'supertypes', symbolName: 'Rounded', line: 4, extras: H }, 'lsp supertypes'],
  ['lspSearch', { path: `${W}/src/shapes.ts`, operation: 'subtypes', symbolName: 'Circle', line: 2, extras: H }, 'lsp subtypes'],
  ['lspSearch', { path: `${W}/src/a.ts`, operation: 'diagnostic', extras: H }, 'lsp diagnostic'],
  ['lspSearch', { symbolName: 'visibleSym', workspaceRoot: WABS, operation: 'workspaceSymbol', extras: H }, 'lsp workspaceSymbol'],
  ['lspSearch', { path: `${W}/src/a.ts`, operation: 'definition', symbolName: 'visibleSym', line: 5, extras: H }, 'lsp anchor miss'],
  ['lspSearch', { path: `${W}/nope.ts`, operation: 'documentSymbols', extras: H }, 'lsp scope miss'],
];

const GH = { owner: 'bgauryy', repo: 'octocode' }; // the canonical name: a renamed repo would make every lead differ from its source
const githubFixtures = [
  ['ghSearchRepo', { keywords: ['octocode'], owner: 'bgauryy', pageSize: 5, extras: H }, 'gh repo search'],
  ['ghSearchRepo', { keywords: ['zzqqnevermatchesrepo12345'], extras: H }, 'gh repo empty'],
  ['ghSearchRepo', { ...GH, extras: H }, 'gh repo facts'],
  ['ghSearchCode', { ...GH, keywords: ['continuation'], pageSize: 3, extras: H }, 'gh code'],
  ['ghSearchCode', { ...GH, keywords: ['continuation'], ref: 'main', pageSize: 3, extras: H }, 'gh code ref'],
  ['ghSearchCode', { ...GH, keywords: ['zzqqnevermatchescode12345'], extras: H }, 'gh code empty'],
  ['ghStructure', { ...GH, path: 'packages', extras: H }, 'gh structure dir'],
  ['ghStructure', { ...GH, path: 'packages/zzqqnope', extras: H }, 'gh structure miss'],
  ['ghStructure', { ...GH, operation: 'refs', extras: H }, 'gh structure refs'],
  ['ghGetFileContent', { ...GH, path: 'README.md', extras: H }, 'gh file'],
  ['ghGetFileContent', { ...GH, path: 'README.md', matchString: 'zzqqnevermatches', extras: H }, 'gh file empty'],
  ['ghGetFileContent', { ...GH, path: 'zzqq/nope.md', extras: H }, 'gh file miss'],
  ['ghSearchHistory', { ...GH, operation: 'pullRequest', state: 'merged', pageSize: 3, extras: H }, 'gh history prs'],
  ['ghSearchHistory', { ...GH, operation: 'commit', pageSize: 3, extras: H }, 'gh history commits'],
  ['ghSearchHistory', { ...GH, operation: 'issue', pageSize: 3, extras: H }, 'gh history issues'],
  ['ghSearchHistory', { ...GH, operation: 'issue', keywords: ['zzqqnevermatcheshistory12345'], extras: H }, 'gh history empty'],
  ['artifactSearch', { ecosystem: 'npm', packageName: 'octocode-mcp', extras: H }, 'artifact npm'],
  ['artifactSearch', { ecosystem: 'crates', packageName: 'tokio', extras: H }, 'artifact crates'],
  ['artifactSearch', { ecosystem: 'npm', packageName: 'zzqq-nope-pkg-12345', extras: H }, 'artifact miss'],
];

// Execution.
const octocodeBin = path.join(ROOT, 'packages/octocode/out/octocode.js');
buildWorkspace();
const client = await startServer({ env: { OCTOCODE_BETA: 'true', PATH: `${path.join(ROOT, 'node_modules', '.bin')}${path.delimiter}${process.env.PATH}` }, timeoutMs: 90_000 });
const mcpTools = new Set(client.tools.map(t => t.name));
const onMcp = name => mcpTools.has(name);
const CLI_ONLY = new Set(); // CLI-only tools reached through the CLI when the MCP server does not list them
const exec = (tool, args, label) => (onMcp(tool) ? client.raw(tool, args, label) : cliEntry(tool, args, { label, timeout: 90_000 }));
// The published MCP schema leaves some fields out (e.g. astSearch includeIgnored); the CLI `schema --view query` lists every accepted field.
const fieldProps = new Map();
function propsOf(tool) {
  if (!fieldProps.has(tool)) {
    const run = spawnSync(process.execPath, [octocodeBin, 'schema', tool, '--view', 'query'], { encoding: 'utf8', timeout: 30_000 });
    try { fieldProps.set(tool, properties(JSON.parse(run.stdout).querySchema)); } catch { fieldProps.set(tool, new Set()); }
  }
  return fieldProps.get(tool);
}
async function pool(items, worker) {
  let i = 0;
  await Promise.all(Array.from({ length: CONCURRENCY }, async () => { while (i < items.length) await worker(items[i++]); }));
}

const contract = JSON.parse(fs.readFileSync(path.join(ROOT, 'packages/octocode-config/contract/tool-contract.json'), 'utf8'));
const PARKED = new Set(contract.tools.filter(t => t.disabled).map(t => t.name));
const plan = fixtures().filter(([tool, , label]) => {
  if (PARKED.has(tool)) {
    skip(`fixture ${label}`, `${tool} is parked (disabled on every surface)`);
    return false;
  }
  if (onMcp(tool) || CLI_ONLY.has(tool)) return true;
  skip(`fixture ${label}`, `${tool} is not registered on this server`);
  return false;
});
// GitHub and registry fixtures are the live part of this suite (they need the network and the caller's GitHub login).
if (LIVE) plan.push(...githubFixtures);
else skip('GitHub and registry fixtures', 'live (set OCTOCODE_LIVE=1): GitHub and registry leads are not replayed');

// A brief on these fixtures probes that no continuation inherits mainGoal/reasoning (one per tool family).
const BRIEF = new Set(['lf large default window', 'ss files paged', 'as symbols dir', 'lsp references', 'gh code', 'gh history prs', 'lf matchString windows']);
const sources = [];
await pool(plan, async ([tool, rawQuery, label]) => {
  const query = BRIEF.has(label) ? { ...rawQuery, mainGoal: 'lead replay probe', reasoning: 'brief inheritance probe' } : rawQuery;
  const args = { queries: [query] };
  sources.push({ label, tool, args, entry: await exec(tool, args, label), depth: 0 });
});
sources.sort((a, b) => plan.findIndex(p => p[2] === a.label) - plan.findIndex(p => p[2] === b.label));

// A stale snapshot: the page-2 lead of a fixture search after the corpus changed must offer next.restart.
{
  const probe = sources.find(s => s.label === 'ls stale source');
  if (probe) probe.noReplay = true; // its own page leads go stale on purpose below
  const page = leadsOf(probe?.entry.sc).find(l => l.key === 'nextPage');
  if (page) {
    fs.appendFileSync(path.join(SABS, 's00.ts'), 'export const staleProbe = 1; // needleStale\n');
    sources.push({ label: 'ls stale snapshot', tool: page.tool, args: page.query, entry: await exec(page.tool, page.query, 'stale page'), depth: 0 });
  }
}

// Judging.
const failures = [];
const replayed = [];
const stats = { sourceCalls: sources.length, leadsSeen: 0, replayed: 0, duplicates: 0, capped: 0, overBudget: 0, skipped: {}, assertions: 0 };

let batch = [];
function record(_lead, _source, assertion, ok, detail) { batch.push({ assertion, ok: !!ok, detail }); stats.assertions += 1; return ok; }
/** One PASS line per lead listing the assertions that held; a FAIL line each for the rest. */
function flush(lead, source) {
  const base = `${source.label} :: ${lead.key} (${lead.container}) -> ${lead.tool}`;
  const passed = batch.filter(b => b.ok).map(b => b.assertion);
  if (passed.length) check(`${base} :: ${passed.join(',')}`, true);
  for (const { assertion, detail } of batch.filter(b => !b.ok)) {
    const name = `${base} :: ${assertion}`;
    const info = { source: source.label, sourceTool: source.tool, sourceQuery: source.args, leadPath: lead.path, key: lead.key, tool: lead.tool, query: lead.query, assertion, outcome: short(detail, 600) };
    failures.push(info);
    check(name, false, detail);
    console.log(`  source: ${source.tool} ${short(canon(source.args), 300)}\n  lead  : ${lead.key} ${short(canon(lead.query), 400)}\n  result: ${short(detail, 400)}`);
  }
  batch = [];
}

/** Judge one executed lead against the predicates of its key family (harness/lead-predicates.mjs). */
function judge(lead, source, entry, shown, { walked = false } = {}) {
  for (const { assertion, ok, detail } of judgeLead(lead, source, entry, shown, { walked, root: ROOT, propsOf, defaultsOf })) record(lead, source, assertion, ok, detail);
  flush(lead, source);
  return outcomeOf(entry);
}

// Count views fill the response window by default (pageSize: "files/count
// views fill the configured response window"): 101 short rows are one
// complete page with no continuation.
{
  const query = { path: PREL, matchString: 'needleCountPages', resultView: 'countMatches', extras: H };
  const entry = await exec('localSearch', { queries: [query] }, 'countMatches default window');
  const data = entry.sc?.results?.[0]?.data;
  const files = new Set((data?.files ?? []).map(file => path.resolve(ROOT, file.path)));
  const nextPage = leadsOf(entry.sc).find(l => l.key === 'nextPage');
  check('countMatches default view returns all 101 files on one page', !outcomeOf(entry).error && files.size === 101 && (data?.files ?? []).length === 101 && !nextPage && data?.pagination?.hasMore !== true, `files=${files.size} next=${!!nextPage} pagination=${JSON.stringify(data?.pagination ?? null)}`);
}

// Explicit countMatches pagination must keep its query scope and return every
// file once, including the file beyond the first 100-file page.
{
  const firstQuery = { path: PREL, matchString: 'needleCountPages', resultView: 'countMatches', pageSize: 100, extras: H };
  let source = { label: 'ls countMatches full union', tool: 'localSearch', args: { queries: [firstQuery] }, entry: await exec('localSearch', { queries: [firstQuery] }, 'countMatches full union'), depth: 0 };
  const expected = new Set(Array.from({ length: 101 }, (_, i) => path.join(PABS, `f${String(i).padStart(3, '0')}.ts`)));
  const seenFiles = new Set();
  const pageSizes = [];
  let pageCount = 0;
  let duplicate = false;
  let allOk = true;
  let next;
  do {
    const row = source.entry.sc?.results?.[0];
    const pagination = row?.data?.pagination;
    allOk &&= !outcomeOf(source.entry).error && row?.status !== 'error' && pagination?.totalItems === 101;
    const files = row?.data?.files ?? [];
    pageSizes.push(files.length);
    for (const file of files) {
      const absolute = path.resolve(ROOT, file.path);
      if (seenFiles.has(absolute)) duplicate = true;
      seenFiles.add(absolute);
    }
    pageCount += 1;
    next = leadsOf(source.entry.sc).find(l => l.key === 'nextPage');
    if (!next) break;
    const sourceQuery = rowOf(source.args);
    const leadQuery = rowOf(next.query);
    const unchanged = scopeChanges(sourceScopeForPage(source, next, sourceQuery, leadQuery), leadQuery, { root: ROOT });
    check('countMatches nextPage keeps scope', unchanged.length === 0, unchanged.join('; '));
    if (pageCount === 1) {
      check('countMatches wrong pageSize still changes scope', scopeChanges(sourceScopeForPage(source, next, sourceQuery, { ...leadQuery, pageSize: 99 }), { ...leadQuery, pageSize: 99 }, { root: ROOT }).some(s => s.startsWith('pageSize:')), 'pageSize 99 must not inherit the reported 100');
      check('countMatches wrong path still changes scope', scopeChanges(sourceScopeForPage(source, next, sourceQuery, { ...leadQuery, path: WREL }), { ...leadQuery, path: WREL }, { root: ROOT }).some(s => s.startsWith('path:')), 'path change must remain visible');
    }
    source = { label: 'ls countMatches full union page', tool: 'localSearch', args: next.query, entry: await exec(next.tool, next.query, 'countMatches full union nextPage'), depth: 0 };
  } while (pageCount < 3);
  check('countMatches pages report complete 101-file union', allOk && !next && pageCount === 2 && pageSizes[0] === 100 && pageSizes[1] === 1 && !duplicate && seenFiles.size === expected.size && [...expected].every(p => seenFiles.has(p)), `pages=${pageCount} sizes=${pageSizes.join(',')} unique=${seenFiles.size} duplicate=${duplicate} next=${!!next} allOk=${allOk}`);
}

// Replay: breadth-first over every distinct lead, bounded per key and by the time budget.
const seen = new Set();
const perKey = new Map();
const queue = [];
const enqueue = (source) => {
  for (const lead of leadsOf(source.entry.sc)) {
    stats.leadsSeen += 1;
    queue.push({ lead, source });
  }
};
for (const s of sources.filter(x => !x.noReplay)) enqueue(s);
const keyStats = new Map();
const bump = (key, field) => { const k = keyStats.get(key) ?? { emitted: 0, replayed: 0 }; k[field] += 1; keyStats.set(key, k); };
queue.forEach(({ lead }) => bump(`${lead.container}.${lead.key}`, 'emitted'));
// Rare keys first, so the time budget cuts the repeated ones (hints.read repeats per hit), not a lone lead.
queue.sort((a, b) => keyStats.get(`${a.lead.container}.${a.lead.key}`).emitted - keyStats.get(`${b.lead.container}.${b.lead.key}`).emitted);

async function replay({ lead, source }) {
  const id = `${lead.tool}:${canon(lead.query)}`;
  const kind = `${lead.tool}:${lead.key}`;
  const why = SKIP_EXEC_KEYS[lead.key];
  if (why) { stats.skipped[lead.key] = (stats.skipped[lead.key] ?? 0) + 1; return; }
  if (seen.has(id)) { stats.duplicates += 1; return; }
  if (Date.now() - STARTED > BUDGET_MS) { stats.overBudget += 1; return; }
  if ((perKey.get(kind) ?? 0) >= PER_KEY) { stats.capped += 1; return; }
  seen.add(id);
  perKey.set(kind, (perKey.get(kind) ?? 0) + 1);
  stats.replayed += 1;
  bump(`${lead.container}.${lead.key}`, 'replayed');
  const entry = await exec(lead.tool, lead.query, `${source.label} > ${lead.key}`);
  const shown = atoms(source.entry.sc);
  const out = judge(lead, source, entry, shown);
  replayed.push({ source: source.label, key: lead.key, container: lead.container, tool: lead.tool, query: lead.query, outcome: out.text, ms: entry.ms });
  const child = { label: `${source.label} > ${lead.key}`, tool: lead.tool, args: lead.query, entry, depth: source.depth + 1 };

  // Walk a paging lead on: every further page shows rows not yet shown and keeps the scope.
  if (PAGE_KEYS.test(lead.key) && lead.key !== 'responsePagination.next' && !out.error) {
    const everShown = new Set([...shown, ...atoms(entry.sc)]);
    let current = child;
    for (let n = 2; n <= WALK_PAGES + 1; n++) {
      const next = leadsOf(current.entry.sc).find(l => l.key === lead.key);
      if (!next || seen.has(`${next.tool}:${canon(next.query)}`)) break;
      seen.add(`${next.tool}:${canon(next.query)}`);
      const walked = await exec(next.tool, next.query, `${child.label} page ${n}`);
      const step = { ...next, path: `${lead.path}#page${n}` };
      judge(step, current, walked, everShown, { walked: true });
      stats.replayed += 1;
      atoms(walked.sc).forEach(a => everShown.add(a));
      current = { label: `${child.label} page ${n}`, tool: next.tool, args: next.query, entry: walked, depth: child.depth };
      if (outcomeOf(walked).error) break;
    }
  }
  if (child.depth < MAX_DEPTH) { const before = queue.length; enqueue(child); queue.slice(before).forEach(({ lead: l }) => bump(`${l.container}.${l.key}`, 'emitted')); }
}

for (let cursor = 0; cursor < queue.length;) {
  const batch = queue.slice(cursor, cursor + CONCURRENCY).filter(item => item.lead.key !== 'apply');
  cursor += CONCURRENCY;
  await Promise.all(batch.map(replay));
}
for (const item of queue.filter(item => item.lead.key === 'apply')) await replay(item);

// Report.
const contractKeys = new Set(Object.values(contract.continuationChannels?.kinds ?? {}).flat());
const emitted = new Set([...keyStats.keys()].map(k => k.split('.').slice(1).join('.')));
const unexercised = [...contractKeys].filter(k => !emitted.has(k)).sort();
if (stats.overBudget) skip('replay budget', `${stats.overBudget} leads not replayed: the ${BUDGET_MS} ms budget ended (raise OCTOCODE_LEAD_REPLAY_BUDGET_MS)`);
for (const [key, n] of Object.entries(stats.skipped)) skip(`lead ${key}`, `${n} not executed: ${SKIP_EXEC_KEYS[key]}`);
console.log(`\n[${SUITE}] ${stats.sourceCalls} source calls, ${stats.leadsSeen} leads seen, ${stats.replayed} replayed, ${stats.duplicates} duplicates, ${stats.capped} over the per-key cap (${PER_KEY}), ${stats.overBudget} over budget, ${(Date.now() - STARTED) / 1000}s`);
console.log(`[${SUITE}] contract lead keys no fixture emitted: ${unexercised.join(', ') || 'none'}`);
const keyTable = [...keyStats].sort(([a], [b]) => (a < b ? -1 : 1)).map(([k, v]) => `${k} ${v.replayed}/${v.emitted}`).join(', ');
console.log(`[${SUITE}] replayed/emitted per key: ${keyTable}`);

const result = summary();
writeResults(SUITE, { stats, keyStats: Object.fromEntries(keyStats), unexercised, failures, replayed, sources: sources.map(s => ({ label: s.label, tool: s.tool, args: s.args, ms: s.entry.ms, outcome: outcomeOf(s.entry).text })), results: result.results });
client.close();
if (!process.env.OCTOCODE_LEAD_REPLAY_KEEP) { fs.rmSync(WABS, { recursive: true, force: true }); fs.rmSync(SABS, { recursive: true, force: true }); fs.rmSync(PABS, { recursive: true, force: true }); }
process.exitCode = result.failed.length ? 1 : 0;
