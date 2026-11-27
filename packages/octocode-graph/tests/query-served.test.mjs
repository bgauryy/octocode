// `octocode-graph query` on a run its live viewer serves: the server answers from the graph it holds; with no live
// server, the graph is read here. Either way the answer warns of graph files changed since it was written.
import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { createServer } from 'node:http';
import { writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { promisify } from 'node:util';
import test from 'node:test';
import { tempDir } from './test-tmp.mjs';

const run = promisify(execFile);
const query = (dir, text) =>
  run(process.execPath, ['--input-type=module', '-e', `import { main } from ${JSON.stringify(new URL('../src/runtime/query.ts', import.meta.url).href)}; await main(${JSON.stringify([dir, text, '--json'])});`], { encoding: 'utf8' });

function graphDir() {
  const dir = tempDir('graph-served-');
  // The graph's one file does not exist under its root: it counts as changed since the graph was written.
  writeFileSync(join(dir, 'data.json'), JSON.stringify({ section: join(dir, 'src'), files: [{ id: 'a.ts', label: 'a.ts' }], imports: [], symbols: [], calls: [] }));
  return dir;
}

test('a served run is answered by its viewer; a run whose viewer is gone is answered from the graph here', async (t) => {
  const dir = graphDir();
  const asked = [];
  const server = createServer((req, res) => {
    let body = '';
    req.on('data', (d) => (body += d)).on('end', () => {
      asked.push({ url: req.url, body: JSON.parse(body) });
      res.setHeader('content-type', 'application/json');
      res.end(JSON.stringify({ ok: true, op: 'search', summary: 'answered by the viewer', rows: [], nodes: [], edges: [] }));
    });
  });
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  t.after(() => server.close());
  const { port } = server.address();
  writeFileSync(join(dir, 'server.json'), JSON.stringify({ url: `http://127.0.0.1:${port}/`, token: 'tok', port, pid: process.pid, out: dir, root: null }));
  const served = JSON.parse((await query(dir, 'search a')).stdout);
  assert.equal(served.summary, 'answered by the viewer');
  assert.deepEqual(asked.map((a) => [a.url, a.body.query]), [['/query?t=tok', 'search a']]);

  // A server.json whose process is gone names no server: the graph is read here, with the stale warning.
  writeFileSync(join(dir, 'server.json'), JSON.stringify({ url: `http://127.0.0.1:${port}/`, token: 'tok', port, pid: 2 ** 22 + 7, out: dir, root: null }));
  const local = JSON.parse((await query(dir, 'search a')).stdout);
  assert.notEqual(local.summary, 'answered by the viewer');
  assert.deepEqual(local.staleFiles, ['a.ts']);
  assert.match(local.warnings[0], /1 graph file changed or missing since this graph was written \(first: a\.ts\)/);
  assert.equal(asked.length, 1);
});
