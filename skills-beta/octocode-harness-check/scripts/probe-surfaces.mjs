#!/usr/bin/env node
// Live input/output probe of every Octocode tool across native CLI, node CLI, and
// MCP stdio. Each case has a deterministic content check. Records result class,
// exit code, output chars, latency, `next.*` byte share, and the MCP catalog weight.
import { spawn, spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

const args = process.argv.slice(2);
const opt = (name, fallback) => {
  const i = args.indexOf(`--${name}`);
  return i >= 0 ? args[i + 1] : fallback;
};
if (args.includes('--help')) {
  console.log(`probe-surfaces — run the tool I/O matrix on native CLI, node CLI, and MCP

  node scripts/probe-surfaces.mjs --out <dir> [--repo <octocode checkout>] [--only <regex>]
       [--surfaces native,node,mcp] [--cases <extra-cases.json>] [--no-github]

  --repo      checkout providing binaries (default: cwd). Override single binaries with
              OCTOCODE_NATIVE_BIN, OCTOCODE_NODE_CLI, OCTOCODE_MCP_ENTRY.
  --out       run directory; writes results.json and fixture/
  --cases     JSON array of [id, tool, query, checkRegex] rows appended to the built-ins
  --no-github skip GitHub/registry cases (offline)
Env: GITHUB_TOKEN for gh* cases, OCTOCODE_BETA=true for astTopology/astRewrite,
PATH must contain typescript-language-server for lspSearch.`);
  process.exit(0);
}

const repo = resolve(opt('repo', process.cwd()));
const out = resolve(opt('out', join(process.cwd(), '.octocode', 'harness-check', 'probe')));
const only = opt('only') ? new RegExp(opt('only')) : null;
const surfaces = opt('surfaces', 'native,node,mcp').split(',');
const offline = args.includes('--no-github');
const BIN = process.env.OCTOCODE_NATIVE_BIN ?? join(repo, `packages/octocode-native/npm/${process.platform}-${process.arch}/octocode`);
const NODECLI = process.env.OCTOCODE_NODE_CLI ?? join(repo, 'packages/octocode/out/octocode.js');
const MCP = process.env.OCTOCODE_MCP_ENTRY ?? join(repo, 'packages/octocode-mcp/dist/index.js');

// Fixture: two TS files with one cross-file call, plus a tsconfig so LSP sees a real project.
const FIX = join(out, 'fixture');
mkdirSync(join(FIX, 'src'), { recursive: true });
writeFileSync(join(FIX, 'src/util.ts'), 'export function greet(name: string): string {\n  return `hello ${name}`;\n}\nexport function shout(name: string): string {\n  return greet(name).toUpperCase();\n}\n');
writeFileSync(join(FIX, 'src/index.ts'), 'import { greet, shout } from "./util";\n\nfunction main(): void {\n  const a = greet("world");\n  const b = shout("world");\n  console.log(a, b);\n}\n\nmain();\n');
writeFileSync(join(FIX, 'package.json'), '{ "name": "probe-fixture", "version": "1.0.0", "type": "module" }\n');
writeFileSync(join(FIX, 'tsconfig.json'), '{"compilerOptions":{"strict":true,"module":"esnext","target":"es2022"},"include":["src"]}\n');
// Same sources without tsconfig: tsserver falls back to an inferred project.
// Outside any repo so no ancestor tsconfig or git root changes the LSP workspace.
const LOOSE = mkdtempSync(join(tmpdir(), 'octocode-probe-loose-'));
mkdirSync(join(LOOSE, 'src'), { recursive: true });
for (const f of ['util.ts', 'index.ts']) writeFileSync(join(LOOSE, 'src', f), readFileSync(join(FIX, 'src', f)));
writeFileSync(join(LOOSE, 'package.json'), '{ "name": "probe-loose", "version": "1.0.0", "type": "module" }\n');
// Tools run with cwd=FIX; allow the sibling fixture explicitly.
process.env.ALLOWED_PATHS = [process.env.ALLOWED_PATHS, LOOSE].filter(Boolean).join(':');

const R = (s) => ({ reasoning: s });
const LOCAL = [
  ['ls.basic', 'localSearch', { ...R('find greet'), searchText: 'greet', path: 'src' }, 'util\\.ts[\\s\\S]*index\\.ts|index\\.ts[\\s\\S]*util\\.ts'],
  ['ls.zero', 'localSearch', { ...R('absent'), searchText: 'zzqqxx_absent', path: 'src' }, 'empty|no match'],
  ['ls.badregex', 'localSearch', { ...R('bad regex'), searchText: '(unclosed', path: 'src', regex: 'rust' }, 'regex|unclosed|parse'],
  ['ls.pcre2', 'localSearch', { ...R('lookahead'), searchText: 'gr(?=eet)', path: 'src', regex: 'pcre2' }, 'util\\.ts'],
  ['lf.full', 'localFetch', { ...R('read'), path: 'src/util.ts' }, 'export function greet'],
  ['lf.match', 'localFetch', { ...R('match'), path: 'src/util.ts', matchString: 'toUpperCase', contextLines: 1 }, 'toUpperCase'],
  ['lf.range', 'localFetch', { ...R('range'), path: 'src/util.ts', startLine: 4, endLine: 6 }, 'shout'],
  ['lf.missing', 'localFetch', { ...R('missing'), path: 'src/nope.ts' }, 'not found|notFound|no such'],
  ['as.match', 'astSearch', { ...R('calls'), operation: 'match', path: 'src', pattern: 'greet($A)', langType: 'typescript' }, 'index\\.ts'],
  ['as.symbols', 'astSearch', { ...R('symbols'), operation: 'symbols', path: 'src' }, 'shout'],
  ['as.files', 'astSearch', { ...R('files'), operation: 'files', path: '.', extensions: ['ts'] }, 'util\\.ts'],
  ['as.tree', 'astSearch', { ...R('tree'), operation: 'tree', path: 'src/util.ts', treeKind: 'syntax' }, 'function'],
  ['lsp.refs', 'lspSearch', { ...R('refs'), operation: 'references', uri: join(FIX, 'src/util.ts'), symbolName: 'greet', lineHint: 1 }, 'index\\.ts'],
  ['lsp.def', 'lspSearch', { ...R('def'), operation: 'definition', uri: join(FIX, 'src/index.ts'), symbolName: 'shout', lineHint: 1 }, 'util\\.ts'],
  ['lsp.callers', 'lspSearch', { ...R('callers'), operation: 'callers', uri: join(FIX, 'src/util.ts'), symbolName: 'greet', lineHint: 1 }, 'shout|main'],
  ['lsp.inferred', 'lspSearch', { ...R('refs without tsconfig'), operation: 'references', uri: join(LOOSE, 'src/util.ts'), symbolName: 'greet', lineHint: 1 }, 'inferredProject'],
  ['at.deps', 'astTopology', { ...R('deps'), operation: 'topology', analysis: 'dependencies', path: '.', file: 'src/index.ts' }, 'util'],
  ['ar.preview', 'astRewrite', { ...R('preview'), path: 'src/util.ts', langType: 'typescript', ruleKind: 'pattern', pattern: 'greet($A)', rewrite: 'greet2($A)' }, 'greet2'],
  ['iv.noReason', 'localSearch', { searchText: 'greet', path: 'src' }, 'reasoning'],
  ['iv.unknownKey', 'localSearch', { ...R('typo'), searchText: 'greet', path: 'src', serchText2: 1 }, 'searchText'],
  ['iv.enumTypo', 'astSearch', { ...R('enum'), operation: 'matches', path: 'src', pattern: 'x' }, 'symbols'],
];
const REMOTE = [
  ['af.npm', 'artifactSearch', { ...R('npm'), type: 'npm', packageName: 'left-pad' }, 'left-pad'],
  ['af.kw', 'artifactSearch', { ...R('keyword'), type: 'npm', keywords: ['yaml', 'parser'] }, 'yaml'],
  ['af.missing', 'artifactSearch', { ...R('missing'), type: 'npm', packageName: 'zzqq-no-such-pkg-xx' }, 'not ?found|empty'],
  ['gs.repos', 'ghSearch', { ...R('repos'), operation: 'repositories', keywords: ['octocode'], owner: 'bgauryy' }, 'octocode'],
  ['gs.tree', 'ghSearch', { ...R('tree'), operation: 'tree', owner: 'octocat', repo: 'Hello-World' }, 'README'],
  ['gf.read', 'ghGetFileContent', { ...R('read'), owner: 'octocat', repo: 'Hello-World', path: 'README' }, 'Hello World'],
  ['gf.404', 'ghGetFileContent', { ...R('404'), owner: 'octocat', repo: 'Hello-World', path: 'NOPE.md' }, 'not found|notFound'],
  ['gh.commits', 'ghSearchHistory', { ...R('commits'), operation: 'commit', owner: 'octocat', repo: 'Hello-World' }, 'sha|message'],
  ['gi.pr', 'ghGetHistoryItem', { ...R('pr'), operation: 'pullRequest', owner: 'octocat', repo: 'Hello-World', number: 1 }, 'title'],
  ['gc.clone', 'ghCloneRepo', { ...R('clone'), owner: 'octocat', repo: 'Hello-World' }, 'localPath', { mcp: 'absent' }],
];
const extra = opt('cases') ? JSON.parse(readFileSync(opt('cases'), 'utf8')) : [];
const CASES = [...LOCAL, ...(offline ? [] : REMOTE), ...extra];

const classify = (t) =>
  /outputContractViolation|violates its canonical output contract/.test(t) ? 'FAIL_CONTRACT'
  : /panicked|is not a function|Cannot find module/.test(t) ? 'FAIL_CRASH'
  : /not found","code":-32602|Tool \w+ not found/.test(t) ? 'ABSENT'
  : /toolError|Invalid arguments|invalidInput|Check the query fields|Input validation error/.test(t) ? 'INPUT_ERR'
  : /"status":"error"|status: error/.test(t) ? 'TOOL_ERR'
  : /"status":"empty"|status: empty/.test(t) ? 'EMPTY' : 'OK';

function nextShare(text) {
  let j;
  try { j = JSON.parse(text); } catch { return null; }
  let n = 0;
  const walk = (o) => {
    if (Array.isArray(o)) o.forEach(walk);
    else if (o && typeof o === 'object') for (const [k, v] of Object.entries(o)) k === 'next' ? (n += JSON.stringify(v).length) : walk(v);
  };
  walk(j);
  return +(n / text.length).toFixed(3);
}

function runCli(bin, tool, q) {
  const t0 = performance.now();
  const r = spawnSync(bin[0], [...bin.slice(1), tool, JSON.stringify(q)], { cwd: FIX, encoding: 'utf8', timeout: 120000, env: process.env, maxBuffer: 64 << 20 });
  const text = (r.stdout || '') + (r.stderr || '') + (r.error ? String(r.error.message) : '');
  return { cls: classify(text), exit: r.status, chars: text.length, ms: Math.round(performance.now() - t0), nextShare: nextShare(r.stdout || ''), text };
}

function mcpSession() {
  const c = spawn('node', [MCP], { stdio: ['pipe', 'pipe', 'pipe'], cwd: FIX, env: process.env });
  let buf = '', err = '', id = 0;
  const pend = new Map();
  c.stdin.on('error', () => {});
  c.stderr.on('data', (d) => { err += d; });
  c.stdout.on('data', (d) => {
    buf += d;
    const lines = buf.split('\n');
    buf = lines.pop();
    for (const l of lines) {
      let m;
      try { m = JSON.parse(l); } catch { continue; }
      pend.get(m.id)?.(m);
      pend.delete(m.id);
    }
  });
  c.on('exit', (code) => { for (const [k, res] of pend) { pend.delete(k); res({ error: { message: `MCP exited ${code}: ${err.slice(0, 500)}` } }); } });
  const call = (method, params) => new Promise((res) => {
    const i = ++id;
    if (c.exitCode !== null) return res({ error: { message: `MCP dead: ${err.slice(0, 500)}` } });
    pend.set(i, res);
    c.stdin.write(`${JSON.stringify({ jsonrpc: '2.0', id: i, method, params })}\n`);
    setTimeout(() => { if (pend.has(i)) { pend.delete(i); res({ error: { message: 'TIMEOUT' } }); } }, 120000);
  });
  return { call, kill: () => c.kill() };
}

const results = [];
let catalog = null;
let mcp = null;
if (surfaces.includes('mcp')) {
  if (!existsSync(MCP)) throw new Error(`MCP entry not found: ${MCP}`);
  mcp = mcpSession();
  const init = await mcp.call('initialize', { protocolVersion: '2025-06-18', capabilities: {}, clientInfo: { name: 'harness-check', version: '1' } });
  if (init.error) {
    console.error(`MCP failed to start: ${init.error.message}`);
    catalog = { startError: init.error.message };
    mcp = null;
  } else {
    const list = await mcp.call('tools/list', {});
    const tools = (list.result?.tools ?? []).map((t) => ({ name: t.name, descChars: (t.description ?? '').length, inputSchemaChars: JSON.stringify(t.inputSchema).length, totalChars: JSON.stringify(t).length }));
    catalog = { instructionsChars: (init.result?.instructions ?? '').length, toolCount: tools.length, totalChars: tools.reduce((a, t) => a + t.totalChars, 0), tools };
  }
}

for (const [id, tool, query, check, expect = {}] of CASES) {
  if (only && !only.test(id)) continue;
  const re = new RegExp(check, 'i');
  const row = { id, tool, inputChars: JSON.stringify(query).length };
  for (const [surf, bin] of [['native', [BIN]], ['node', ['node', NODECLI]]]) {
    if (!surfaces.includes(surf)) continue;
    const r = runCli(bin, tool, query);
    row[surf] = { cls: r.cls, exit: r.exit, chars: r.chars, ms: r.ms, nextShare: r.nextShare, check: re.test(r.text) };
    if (surf === 'native') row.nativeOut = r.text.slice(0, 8000);
  }
  if (mcp) {
    const t0 = performance.now();
    const resp = await mcp.call('tools/call', { name: tool, arguments: { queries: [query] } });
    const text = resp.error ? JSON.stringify(resp.error) : (resp.result?.content ?? []).map((x) => x.text ?? '').join('\n');
    const cls = classify(text);
    row.mcp = { cls: resp.result?.isError && cls === 'OK' ? 'TOOL_ERR' : cls, chars: text.length, structuredChars: resp.result?.structuredContent ? JSON.stringify(resp.result.structuredContent).length : 0, ms: Math.round(performance.now() - t0), check: expect.mcp === 'absent' ? cls === 'ABSENT' : re.test(text) };
    row.mcpOut = text.slice(0, 8000);
  }
  results.push(row);
  console.log(id.padEnd(15), surfaces.filter((s) => row[s]).map((s) => `${s}:${row[s].cls}/${row[s].check ? 'pass' : 'FAIL'}/${row[s].chars}c/${row[s].ms}ms`).join('  '));
}
mcp?.kill();

const summary = {};
for (const s of surfaces) {
  const rows = results.filter((r) => r[s]);
  summary[s] = { runs: rows.length, checkPass: rows.filter((r) => r[s].check).length,
    contractFails: rows.filter((r) => r[s].cls === 'FAIL_CONTRACT').length, crashes: rows.filter((r) => r[s].cls === 'FAIL_CRASH').length };
}
mkdirSync(out, { recursive: true });
writeFileSync(join(out, 'results.json'), JSON.stringify({ generatedAt: new Date().toISOString(), binaries: { BIN, NODECLI, MCP }, summary, catalog, results }, null, 1));
console.log(`\nsummary ${JSON.stringify(summary)}`);
if (catalog?.startError) console.log(`MCP start error: ${catalog.startError}`);
else if (catalog) console.log(`MCP catalog: ${catalog.toolCount} tools, ${catalog.totalChars} chars (~${Math.round(catalog.totalChars / 4)} tok), instructions ${catalog.instructionsChars} chars`);
console.log(`wrote ${join(out, 'results.json')}`);
