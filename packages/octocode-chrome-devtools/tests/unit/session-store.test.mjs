import assert from 'node:assert/strict';
import {
  appendFileSync,
  existsSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { hostname, tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import test from 'node:test';

const engine = resolve(import.meta.dirname, '../../dist/engine');
const store = await import(join(engine, 'session-store.mjs'));
const capture = await import(join(engine, 'session-capture.mjs'));

function workspace(t) {
  const dir = mkdtempSync(join(tmpdir(), 'octo-session-'));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  return dir;
}

const browser = { incarnation: 'b-1', endpoint: 'http://127.0.0.1:9222' };

test('sessions live under the workspace with opaque, path-safe ids', t => {
  const ws = workspace(t);
  const session = store.createSession(ws, browser);
  assert.match(session.id, /^s-[0-9a-f]{16}$/);
  assert.equal(
    store.sessionDir(ws, session.id),
    join(ws, '.octocode', 'chromedevtools', session.id)
  );
  assert.deepEqual(store.listSessions(ws), [session.id]);
  assert.deepEqual(store.openSession(ws, session.id), session);
  for (const bad of ['../x', 's-../../etc', 'p-0123456789abcdef', '', 7])
    assert.throws(() => store.sessionDir(ws, bad), /invalid s identifier/);
});

test('a manifest from another location or version is refused', t => {
  const ws = workspace(t);
  const { id } = store.createSession(ws, browser);
  const path = join(store.sessionDir(ws, id), 'session.json');
  const manifest = JSON.parse(readFileSync(path, 'utf8'));
  writeFileSync(path, JSON.stringify({ ...manifest, version: 99 }));
  assert.throws(() => store.openSession(ws, id), /store version 99/);
  writeFileSync(path, JSON.stringify({ ...manifest, workspace: '/elsewhere' }));
  assert.throws(
    () => store.openSession(ws, id),
    /does not describe its location/
  );
});

test('one writer lease: a live holder refuses, a gone holder is replaced', t => {
  const ws = workspace(t);
  const { id } = store.createSession(ws, browser);
  const lease = store.acquireLease(ws, id, 'b-1');
  assert.throws(() => store.acquireLease(ws, id, 'b-1'), /is leased by pid/);
  assert.equal(store.releaseLease(ws, id, { ...lease, token: 'other' }), false);
  assert.equal(store.releaseLease(ws, id, lease), true);
  assert.equal(store.readLease(ws, id), null);
  // A same-host holder whose process is gone is stale.
  writeFileSync(
    join(store.sessionDir(ws, id), '_lease.json'),
    JSON.stringify({ ...lease, pid: 2 ** 22 + 12345, host: hostname() })
  );
  assert.equal(store.readLease(ws, id).stale, true);
  const next = store.acquireLease(ws, id, 'b-2');
  assert.equal(next.incarnation, 'b-2');
  // Another host's holder cannot be proven gone.
  writeFileSync(
    join(store.sessionDir(ws, id), '_lease.json'),
    JSON.stringify({ ...lease, pid: 2 ** 22 + 12345, host: 'elsewhere' })
  );
  assert.throws(() => store.acquireLease(ws, id, 'b-3'), /is leased by pid/);
});

test('captures publish atomically and verify against their manifest', t => {
  const ws = workspace(t);
  const { id: session } = store.createSession(ws, browser);
  const page = capture.createPage({ workspace: ws, session }, 'target-1');
  assert.deepEqual(store.openSession(ws, session).pages, [page.id]);
  const document = capture.beginDocument(
    { workspace: ws, session },
    page.id,
    'https://a.test/'
  );
  const published = capture.publishCapture(
    { workspace: ws, session, page: page.id, document },
    {
      kind: 'snapshot',
      sourceUrl: 'https://a.test/',
      complete: true,
      files: [
        { name: 'snapshot.json', bytes: () => '{"nodes":[]}' },
        { name: 'text.txt', bytes: () => 'hello' },
      ],
    }
  );
  assert.equal(published.files.length, 2);
  assert.deepEqual(
    capture.readCapture(
      { workspace: ws, session, page: page.id, document },
      published.id
    ).files,
    published.files
  );
  const dir = join(
    store.sessionDir(ws, session),
    page.id,
    'documents',
    document,
    'captures',
    published.id
  );
  writeFileSync(join(dir, 'text.txt'), 'tampered');
  assert.throws(
    () =>
      capture.readCapture(
        { workspace: ws, session, page: page.id, document },
        published.id
      ),
    /does not match its manifest/
  );
});

test('a failed capture publishes nothing and leaves the page unchanged', t => {
  const ws = workspace(t);
  const { id: session } = store.createSession(ws, browser);
  const page = capture.createPage({ workspace: ws, session }, 'target-1');
  const document = capture.beginDocument(
    { workspace: ws, session },
    page.id,
    'https://a.test/'
  );
  assert.throws(
    () =>
      capture.publishCapture(
        { workspace: ws, session, page: page.id, document },
        {
          kind: 'screenshot',
          sourceUrl: 'https://a.test/',
          complete: true,
          files: [
            { name: 'a.png', bytes: () => 'ok' },
            {
              name: 'b.png',
              bytes: () => {
                throw Error('worker died');
              },
            },
          ],
        }
      ),
    /worker died/
  );
  const captures = join(
    store.sessionDir(ws, session),
    page.id,
    'documents',
    document,
    'captures'
  );
  assert.deepEqual(readdirSync(captures), []);
  assert.deepEqual(
    capture.abandonedStaging({
      workspace: ws,
      session,
      page: page.id,
      document,
    }),
    []
  );
  const manifest = JSON.parse(
    readFileSync(
      join(store.sessionDir(ws, session), page.id, 'page.json'),
      'utf8'
    )
  );
  assert.deepEqual(manifest.captures, []);
});

test('capture file names and document epochs are validated', t => {
  const ws = workspace(t);
  const { id: session } = store.createSession(ws, browser);
  const page = capture.createPage({ workspace: ws, session }, 'target-1');
  const document = capture.beginDocument(
    { workspace: ws, session },
    page.id,
    'https://a.test/'
  );
  assert.throws(
    () =>
      capture.publishCapture(
        { workspace: ws, session, page: page.id, document },
        {
          kind: 'x',
          sourceUrl: 'u',
          complete: true,
          files: [{ name: '../escape', bytes: () => '' }],
        }
      ),
    /invalid capture file name/
  );
  assert.throws(
    () =>
      capture.publishCapture(
        {
          workspace: ws,
          session,
          page: page.id,
          document: 'd-0000000000000000',
        },
        {
          kind: 'x',
          sourceUrl: 'u',
          complete: true,
          files: [],
        }
      ),
    /is not an epoch/
  );
});

test('a journal reports an incomplete final record instead of dropping it', t => {
  const ws = workspace(t);
  const path = join(ws, 'steps.jsonl');
  assert.equal(capture.appendJournal(path, { step: 'start' }), 0);
  assert.equal(capture.appendJournal(path, { step: 'done' }), 1);
  const intact = capture.readJournal(path);
  assert.equal(intact.records.length, 2);
  assert.equal(intact.partialAt, null);
  const before = readFileSync(path).byteLength;
  appendFileSync(path, '{"seq":2,"step":"cli');
  const torn = capture.readJournal(path);
  assert.equal(torn.records.length, 2);
  assert.equal(torn.partialAt, before);
});

test('prune planning keeps active and leased sessions and removes only planned ones', t => {
  const ws = workspace(t);
  const old = new Date(Date.now() - 10 * 86_400_000);
  const active = store.createSession(ws, browser, old);
  const closed = store.createSession(ws, browser, old);
  store.updateSession(ws, closed.id, m => (m.lifecycle = 'closed'), old);
  const leased = store.createSession(ws, browser, old);
  store.updateSession(ws, leased.id, m => (m.lifecycle = 'closed'), old);
  store.acquireLease(ws, leased.id, 'b-1');
  const plan = capture.planPrune(ws, 86_400_000);
  assert.deepEqual(
    plan.remove.map(row => row.session),
    [closed.id]
  );
  const kept = Object.fromEntries(
    plan.keep.map(row => [row.session, row.reason])
  );
  assert.equal(kept[active.id], 'active');
  assert.match(kept[leased.id], /leased by pid/);
  assert.ok(
    existsSync(store.sessionDir(ws, closed.id)),
    'planning deletes nothing'
  );
  assert.deepEqual(capture.applyPrune(ws, plan), [closed.id]);
  assert.ok(!existsSync(store.sessionDir(ws, closed.id)));
  assert.ok(existsSync(store.sessionDir(ws, leased.id)));
});
