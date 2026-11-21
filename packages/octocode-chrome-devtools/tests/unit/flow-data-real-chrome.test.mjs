// The data flows (dom-snapshot, system-info, storage-quota, indexeddb-dump, cache-storage-dump,
// service-worker-state, network-mock, block-urls, acknowledged timezone) run through `flow run` on a
// real isolated headless Chrome against a loopback child-process site. Skipped without Chrome.
import test from 'node:test';
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import {
  existsSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { delimiter, join } from 'node:path';
import { freePort } from '../fixtures/free-port.mjs';

const bin = join(import.meta.dirname, '../../bin/octocode-chrome-devtools.mjs');
const chromeCandidates = [
  '/Applications/Google Chrome.app',
  ...process.env.PATH.split(delimiter).flatMap(dir =>
    ['google-chrome', 'chromium', 'chrome'].map(name => join(dir, name))
  ),
];
const skip = !chromeCandidates.some(existsSync) && 'no Chrome installed';

const page = `<!doctype html><title>data-flows</title><p id=p>data</p>
<script>
  localStorage.setItem('k', 'v');
  const open = indexedDB.open('appdb', 1);
  open.onupgradeneeded = () => {
    const store = open.result.createObjectStore('items', { keyPath: 'id' });
    store.put({ id: 1, label: 'first-record' });
  };
  caches.open('c1').then(cache => cache.put('/cached', new Response('cached-body')));
</script>`;
const serverSource = `
require('node:http').createServer((req, res) => {
  if (req.url === '/api') { res.setHeader('content-type', 'text/plain'); return res.end('real'); }
  res.setHeader('content-type', 'text/html');
  res.end(${JSON.stringify(page)});
}).listen(0, '127.0.0.1', function () { console.log(this.address().port); });`;

test(
  'data flows act on a real Chrome and save their evidence',
  { skip, timeout: 280000 },
  async () => {
    const cwd = mkdtempSync(join(tmpdir(), 'octo-flow-data-'));
    const web = spawn(process.execPath, ['-e', serverSource], {
      stdio: ['ignore', 'pipe', 'inherit'],
    });
    const origin = `http://127.0.0.1:${await new Promise(resolve =>
      web.stdout.once('data', chunk => resolve(String(chunk).trim()))
    )}`;
    const port = await freePort();
    const cli = (command, input) => {
      writeFileSync(join(cwd, 'in.json'), JSON.stringify(input));
      const r = spawnSync(
        process.execPath,
        [bin, '/cli', command, '--input', 'in.json', '--json'],
        { cwd, encoding: 'utf8' }
      );
      const parsed = JSON.parse(r.stdout || r.stderr);
      return parsed.structuredContent ?? parsed;
    };
    /** Runs a flow; returns its saved files (name -> parsed JSON or text) and its stdout. */
    const flow = (name, params) => {
      const r = spawnSync(
        process.execPath,
        [
          bin,
          '/raw',
          'flow',
          'run',
          name,
          '--params',
          JSON.stringify(params),
          '--port',
          String(port),
        ],
        { cwd, encoding: 'utf8' }
      );
      const text = r.stdout + r.stderr;
      assert.match(text, /completed=\d+\/\d+/, `${name}: ${text.slice(-600)}`);
      const saved = {};
      for (const [, file] of text.matchAll(
        /\[ARTIFACT\] (?:SAVED_\S+|\S+) (\S*(?:\/saved\/[^\s/]+|events-[\w-]+\.jsonl))\n/g
      ))
        saved[file.split('/').pop()] = readFileSync(file, 'utf8');
      const json = file => JSON.parse(saved[file]);
      return { text, saved, json };
    };
    try {
      assert.equal(
        cli('open', { port, headless: true, url: `${origin}/` }).ok,
        true
      );
      // The page writes IndexedDB and CacheStorage on load: wait until they exist.
      for (let attempt = 0; attempt < 50; attempt++) {
        const probe = flow('storage-quota', { origin });
        if (probe.json('storage-quota.json').usage > 0) break;
        await new Promise(resolve => setTimeout(resolve, 200));
      }

      const snapshot = flow('dom-snapshot', { computedStyle: 'display' });
      const dom = snapshot.json('dom-snapshot.json');
      assert.ok(dom.documents.length >= 1 && dom.strings.length > 0);
      assert.ok(
        dom.strings.includes('data-flows') || dom.strings.includes('data')
      );
      assert.ok(dom.documents[0].layout.nodeIndex.length > 0, 'layout rows');

      const system = flow('system-info', {}).json('system-info.json');
      assert.ok('gpu' in system && 'modelName' in system);

      const quota = flow('storage-quota', { origin }).json(
        'storage-quota.json'
      );
      assert.equal(typeof quota.quota, 'number');
      assert.ok(quota.usage > 0, JSON.stringify(quota));
      assert.ok(Array.isArray(quota.usageBreakdown));

      const idb = flow('indexeddb-dump', {
        origin,
        pageSize: 10,
        acknowledge: true,
      });
      assert.deepEqual(idb.json('idb-databases.json').databaseNames, ['appdb']);
      assert.equal(
        idb.json('idb-database.json').databaseWithObjectStores.objectStores[0]
          .name,
        'items'
      );
      const records = idb.json('idb-data.json');
      assert.equal(records.hasMore, false);
      assert.match(
        JSON.stringify(records.objectStoreDataEntries),
        /first-record/
      );

      const cache = flow('cache-storage-dump', {
        origin,
        pageSize: 10,
        acknowledge: true,
      });
      assert.equal(cache.json('cache-names.json').caches[0].cacheName, 'c1');
      assert.equal(
        cache.json('cache-entries.json').cacheDataEntries[0].requestURL,
        `${origin}/cached`
      );

      // The wait step times out by design: the run still completes, the event files exist.
      const workers = flow('service-worker-state', { ms: 300 });
      assert.ok(workers.text.includes('failedOptional=1'));

      // Request mocking: the page's fetch is paused, then answered by the flow.
      const mock = flow('network-mock', {
        pattern: `${origin}/api`,
        status: 201,
        body: Buffer.from('mocked').toString('base64'),
        trigger: "fetch('/api')",
        ms: 8000,
        acknowledge: true,
      });
      const paused = JSON.parse(
        mock.saved['events-paused.jsonl'].trim().split('\n')[0]
      );
      assert.equal(paused.params.request.url, `${origin}/api`);

      const block = flow('block-urls', {
        pattern: `${origin}/*.css`,
        acknowledge: true,
      });
      assert.match(block.text, /completed=2\/2/);

      const zone = flow('emulate-timezone-locale', {
        timezone: 'Asia/Tokyo',
        locale: 'ja-JP',
        acknowledge: true,
      });
      assert.match(zone.text, /completed=2\/2/);
    } finally {
      cli('cleanup', { port });
      web.kill();
      rmSync(cwd, { recursive: true, force: true });
    }
  }
);
