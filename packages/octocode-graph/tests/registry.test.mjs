// The per-user registry of built graphs: a project directory finds a graph built with --out elsewhere, after the
// default places, newest first; entries whose result directory is gone are pruned; a registered graph answers only
// for its own root. Hermetic: every registry lives in a temp OCTOCODE_HOME.
import assert from 'node:assert/strict';
import { execFile, spawn, spawnSync } from 'node:child_process';
import { once } from 'node:events';
import { createServer } from 'node:http';
import { linkSync, mkdirSync, readdirSync, rmSync, utimesSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { createInterface } from 'node:readline';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';
import test from 'node:test';
import { tempDir } from './test-tmp.mjs';
import { registerRun, registeredRuns, registryHome } from '../src/runtime/registry.ts';

const commands = fileURLToPath(new URL('../src/runtime/commands/', import.meta.url));
const run = promisify(execFile);

// A saved result of `root` in `out`: its events header names the root, its graph one file of it.
function savedRun(root, out, { age = 0, files = ['a.ts'] } = {}) {
  mkdirSync(out, { recursive: true });
  writeFileSync(join(out, 'events.jsonl'), `${JSON.stringify({ seq: 0, t: 0, type: 'run', root, name: 'p', out, options: { lsp: 'off' } })}\n`);
  writeFileSync(join(out, 'data.json'), JSON.stringify({ section: root, files: files.map((id) => ({ id, label: id })), imports: [], symbols: [], calls: [] }));
  // An older graph: a frozen clock, never wall time.
  const t = new Date(Date.UTC(2026, 0, 1) - age * 1000);
  utimesSync(join(out, 'data.json'), t, t);
  return out;
}

function project() {
  const base = tempDir('graph-registry-');
  const root = join(base, 'project');
  mkdirSync(root);
  writeFileSync(join(root, 'a.ts'), 'export const a = 1;\n');
  utimesSync(join(root, 'a.ts'), new Date(Date.UTC(2025, 0, 1)), new Date(Date.UTC(2025, 0, 1)));
  const home = join(base, 'home');
  const elsewhere = join(base, 'elsewhere');
  mkdirSync(elsewhere);
  return { base, root, home, elsewhere, env: { ...process.env, OCTOCODE_HOME: home } };
}

const entries = (home) => readdirSync(registryHome({ OCTOCODE_HOME: home }), { recursive: true }).filter((p) => String(p).endsWith('.json'));

test('the registry lives in the Octocode home: OCTOCODE_HOME, else ~/.octocode', () => {
  assert.equal(registryHome({ OCTOCODE_HOME: '/x/home' }), join('/x/home', 'graph-registry'));
  assert.match(registryHome({}), /[\\/]\.octocode[\\/]graph-registry$/);
});

test('registered runs are kept per root, and a run whose directory is gone is pruned', () => {
  const p = project();
  const home = registryHome(p.env);
  const a = savedRun(p.root, join(p.base, 'out-a'));
  const b = savedRun(p.root, join(p.base, 'out-b'));
  const other = savedRun(p.elsewhere, join(p.base, 'out-other'));
  registerRun(p.root, a, home);
  registerRun(p.root, b, home);
  registerRun(p.root, b, home);
  registerRun(p.elsewhere, other, home);
  assert.deepEqual(registeredRuns(p.root, home).toSorted(), [a, b]);
  assert.deepEqual(registeredRuns(p.elsewhere, home), [other]);
  rmSync(a, { recursive: true });
  assert.deepEqual(registeredRuns(p.root, home), [b]);
  assert.equal(entries(p.home).length, 2, 'the entry of the removed run is gone');
});

test('query <project> answers from the newest registered graph built elsewhere; stderr names it', async () => {
  const p = project();
  const home = registryHome(p.env);
  const older = savedRun(p.root, join(p.base, 'older'), { age: 60 });
  const newer = savedRun(p.root, join(p.base, 'newer'), { files: ['a.ts', 'b.ts'] });
  registerRun(p.root, newer, home);
  registerRun(p.root, older, home);
  const r = await run(process.execPath, [join(commands, 'query.ts'), p.root, 'search a', '--json'], { cwd: p.elsewhere, env: p.env, encoding: 'utf8' });
  assert.match(r.stderr, new RegExp(`answering from ${newer.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')} \\(registered`));
  assert.equal(JSON.parse(r.stdout).ok, true);

  // The default places come first: a run under the project's .octocode/octocode-graph answers even when older.
  const local = savedRun(p.root, join(p.root, '.octocode', 'octocode-graph', 'local'), { age: 3600 });
  const l = await run(process.execPath, [join(commands, 'query.ts'), p.root, 'search a', '--json'], { cwd: p.elsewhere, env: p.env, encoding: 'utf8' });
  assert.match(l.stderr, new RegExp(`answering from ${local.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}`));
});

test('a registered graph answers only for its own root', () => {
  const p = project();
  // An entry naming a result whose own events header is another root's: never chosen.
  const foreign = savedRun(p.elsewhere, join(p.base, 'foreign'));
  registerRun(p.root, foreign, registryHome(p.env));
  const r = spawnSync(process.execPath, [join(commands, 'query.ts'), p.root, 'search a'], { cwd: p.elsewhere, env: p.env, encoding: 'utf8' });
  assert.equal(r.status, 2);
  assert.match(r.stderr, /no saved graph of .* or the graph registry/);
});

test('openGraph, diff and refresh of a project directory consult the registry', async () => {
  const p = project();
  const out = savedRun(p.root, join(p.base, 'result'));
  registerRun(p.root, out, registryHome(p.env));
  const api = new URL('../src/runtime/api.ts', import.meta.url).href;
  const opened = await run(process.execPath, ['--input-type=module', '-e', `import { openGraph } from ${JSON.stringify(api)}; console.log(openGraph(${JSON.stringify(p.root)}).run);`], { cwd: p.elsewhere, env: p.env, encoding: 'utf8' });
  assert.equal(opened.stdout.trim(), out);

  // diff <project>: the graph its last refresh replaced (previous.json) against now.
  linkSync(join(out, 'data.json'), join(out, 'previous.json'));
  const diff = spawnSync(process.execPath, [join(commands, 'diff.ts'), p.root, '--json'], { cwd: p.elsewhere, env: p.env, encoding: 'utf8' });
  assert.equal(diff.status, 0, diff.stderr);
  assert.deepEqual(JSON.parse(diff.stdout).edges.added, []);

  // refresh <project>: the registered run refreshes (here through the viewer serving it).
  const asked = [];
  const server = createServer((req, res) => {
    asked.push(req.url);
    res.setHeader('content-type', 'application/json');
    res.end(JSON.stringify({ ok: true, ms: 5, chunks: [], changed: [] }));
  });
  await new Promise((ok) => server.listen(0, '127.0.0.1', ok));
  const port = server.address().port;
  writeFileSync(join(out, 'server.json'), JSON.stringify({ pid: process.pid, url: `http://127.0.0.1:${port}/`, token: 'tok', port, out, root: p.root }));
  const child = spawn(process.execPath, [join(commands, 'refresh.ts'), p.root], { cwd: p.elsewhere, env: p.env, stdio: ['ignore', 'pipe', 'pipe'] });
  let stdout = '', stderr = '';
  child.stdout.on('data', (d) => (stdout += d));
  child.stderr.on('data', (d) => (stderr += d));
  const [code] = await once(child, 'exit');
  server.close();
  assert.equal(code, 0, stderr);
  assert.deepEqual(asked, ['/refresh?t=tok']);
  assert.equal(JSON.parse(stdout).out, out);
});

test('MCP query_graph with a project directory answers from its registered graph', { timeout: 30000 }, async (t) => {
  const p = project();
  const out = savedRun(p.root, join(p.base, 'result'));
  registerRun(p.root, out, registryHome(p.env));
  const child = spawn(process.execPath, [join(commands, 'mcp.ts')], { cwd: p.elsewhere, env: p.env, stdio: ['pipe', 'pipe', 'ignore'] });
  t.after(() => child.kill());
  const replies = new Map();
  createInterface({ input: child.stdout }).on('line', (line) => {
    const m = JSON.parse(line);
    replies.get(m.id)?.(m);
  });
  const ask = (id, method, params) => new Promise((ok) => {
    replies.set(id, ok);
    child.stdin.write(`${JSON.stringify({ jsonrpc: '2.0', id, method, params })}\n`);
  });
  await ask(1, 'initialize', { protocolVersion: '2025-06-18', capabilities: {} });
  const q = await ask(2, 'tools/call', { name: 'query_graph', arguments: { result: p.root, query: 'search a', format: 'json' } });
  assert.notEqual(q.result.isError, true, q.result.content[0].text);
  assert.equal(JSON.parse(q.result.content[0].text).ok, true);
});
