// storage-dump on a real isolated headless Chrome: a known localStorage key, an IndexedDB store
// with a known record count, a CacheStorage entry and a cookie are found; values stay out of
// stdout and out of the default JSON, and appear only with includeValues in a 0600 file.
import test from 'node:test';
import assert from 'node:assert/strict';
import { execFile, spawn } from 'node:child_process';
import { promisify } from 'node:util';
import {
  existsSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  statSync,
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

const dist = join(import.meta.dirname, '../../dist/engine');
const RUNNER = `
import { connectCDP } from ${JSON.stringify(join(dist, 'cdp-connection.mjs'))};
const targets = await (await fetch('http://127.0.0.1:' + process.env.PORT + '/json')).json();
const target = targets.find(t => t.type === 'page');
const cdp = await connectCDP(target.webSocketDebuggerUrl, { targetInfo: target });
cdp.outputDir = process.env.OUT;
const recipe = await import(${JSON.stringify(dist)} + '/cdp-checks/' + process.argv[1] + '-check.mjs');
await recipe.run(cdp);
cdp.close();
process.exit(process.exitCode ?? 0);
`;

const RECORDS = 5;
const VALUES = {
  local: 'sekret-local-value-71',
  idb: 'sekret-idb-value-72',
  cookie: 'sekret-cookie-value-73',
};
const page = `<!doctype html><title>storage</title><script>
localStorage.setItem('known-key', ${JSON.stringify(VALUES.local)});
sessionStorage.setItem('tab-key', 'tab');
(async () => {
  const cache = await caches.open('cache-v1');
  await cache.put('/cached.txt', new Response('cached-body'));
  const open = indexedDB.open('appdb', 1);
  open.onupgradeneeded = () => {
    const store = open.result.createObjectStore('items', { keyPath: 'id' });
    store.createIndex('by-name', 'name', { unique: false });
  };
  open.onsuccess = () => {
    const tx = open.result.transaction('items', 'readwrite');
    for (let i = 1; i <= ${RECORDS}; i++)
      tx.objectStore('items').put({ id: i, name: 'n' + i, secret: ${JSON.stringify(VALUES.idb)} });
    tx.oncomplete = () => (document.title = 'ready');
  };
})();
</script>`;

test(
  'storage-dump reports every storage area with values redacted, and saves values only on request',
  { skip, timeout: 150000 },
  async () => {
    const cwd = mkdtempSync(join(tmpdir(), 'octo-storage-dump-'));
    const web = spawn(
      process.execPath,
      [
        '-e',
        `require('node:http').createServer((_, r) => {
          r.setHeader('content-type', 'text/html');
          r.setHeader('set-cookie', 'sid=${VALUES.cookie}; Path=/; HttpOnly; SameSite=Lax');
          r.end(${JSON.stringify(page)});
        }).listen(0, '127.0.0.1', function () { console.log(this.address().port); });`,
      ],
      { stdio: ['ignore', 'pipe', 'inherit'] }
    );
    const webPort = await new Promise(resolve =>
      web.stdout.once('data', chunk => resolve(Number(chunk)))
    );
    const url = `http://127.0.0.1:${webPort}/`;
    const port = await freePort();
    const cli = async (command, input) => {
      writeFileSync(join(cwd, 'in.json'), JSON.stringify(input));
      const r = await promisify(execFile)(
        process.execPath,
        [bin, '/cli', command, '--input', 'in.json', '--json'],
        { cwd, encoding: 'utf8' }
      ).catch(error => error);
      const parsed = JSON.parse(r.stdout || r.stderr);
      return parsed.structuredContent ?? parsed;
    };
    // The recipe module runs in its own process: its options are read from the environment.
    const dump = async options => {
      const outputDir = mkdtempSync(join(cwd, 'out-'));
      const r = await promisify(execFile)(
        process.execPath,
        ['--input-type=module', '-e', RUNNER, 'storage-dump'],
        {
          encoding: 'utf8',
          env: {
            ...process.env,
            PORT: String(port),
            OUT: outputDir,
            ...options,
          },
        }
      ).catch(error => error);
      const stdout = r.stdout + (r.stderr ?? '');
      const file = /\[ARTIFACT\] STORAGE_DUMP (\S+)/.exec(stdout)?.[1];
      return { stdout, file };
    };
    try {
      assert.equal(
        (await cli('open', { port, headless: true, url: 'about:blank' })).ok,
        true
      );
      // The page fills storage asynchronously: dump until the IndexedDB store is complete.
      await cli('run', {
        connection: { port },
        plan: { steps: [{ op: 'goto', url }] },
      });
      let result;
      for (let attempt = 0; attempt < 20; attempt++) {
        result = await dump({});
        if (result.stdout.includes(`idbEntries=${RECORDS}`)) break;
        await new Promise(r => setTimeout(r, 500));
      }
      const { stdout, file } = result;
      assert.ok(file, String(stdout) + String(result.err));
      assert.ok(file, stdout);
      const saved = JSON.parse(readFileSync(file, 'utf8'));
      const raw = readFileSync(file, 'utf8');

      assert.equal(saved.valuesRedacted, true);
      const known = saved.localStorage.find(e => e.key === 'known-key');
      assert.deepEqual(known.value, {
        redacted: true,
        type: 'string',
        bytes: VALUES.local.length,
      });
      assert.ok(saved.sessionStorage.some(e => e.key === 'tab-key'));
      const store = saved.indexedDb
        .find(db => db.name === 'appdb')
        .objectStores.find(s => s.name === 'items');
      assert.equal(store.entries, RECORDS);
      assert.equal(store.keyPath, 'id');
      assert.deepEqual(
        store.indexes.map(i => i.name),
        ['by-name']
      );
      assert.equal(store.records, undefined, 'records only when asked');
      const cache = saved.cacheStorage.find(c => c.name === 'cache-v1');
      assert.ok(cache.entries.some(e => e.url.endsWith('/cached.txt')));
      const cookie = saved.cookies.find(c => c.name === 'sid');
      assert.equal(cookie.httpOnly, true);
      assert.equal(cookie.value, undefined);
      for (const secret of Object.values(VALUES)) {
        assert.ok(!stdout.includes(secret), 'value in stdout');
        assert.ok(!raw.includes(secret), 'value in default JSON');
      }
      assert.ok(!stdout.includes('SECRET_VALUES_SAVED'));

      // Records only on request, the first N in key order, still redacted.
      const withRecords = await dump({ STORAGE_DUMP_IDB_RECORDS: '2' });
      const recordsSaved = JSON.parse(readFileSync(withRecords.file, 'utf8'));
      assert.deepEqual(recordsSaved.errors, []);
      const rows = recordsSaved.indexedDb[0].objectStores[0].records;
      assert.ok(rows, withRecords.stdout + JSON.stringify(recordsSaved.errors));
      assert.deepEqual(
        rows.map(r => r.key),
        [1, 2]
      );
      assert.equal(rows[0].value.redacted, true);

      // includeValues: values land in the 0600 file only.
      const full = await dump({
        STORAGE_DUMP_INCLUDE_VALUES: '1',
        STORAGE_DUMP_IDB_RECORDS: '1',
      });
      assert.match(
        full.stdout,
        /\[FINDING\] SECRET_VALUES_SAVED .*storage-dump\.json/
      );
      for (const secret of Object.values(VALUES))
        assert.ok(
          !full.stdout.includes(secret),
          'value in stdout with includeValues'
        );
      const fullRaw = readFileSync(full.file, 'utf8');
      for (const secret of Object.values(VALUES))
        assert.ok(
          fullRaw.includes(secret),
          `${secret} missing from saved values`
        );
      assert.equal(statSync(full.file).mode & 0o777, 0o600);
    } finally {
      try {
        await cli('cleanup', { port });
      } finally {
        web.kill();
        rmSync(cwd, { recursive: true, force: true });
      }
    }
  }
);
