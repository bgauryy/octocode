import { test, beforeEach, afterEach } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, rmSync, mkdirSync, writeFileSync, readFileSync, readdirSync, statSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { concatCleanParts, redactSecretsInText, redactHarObject } from '../lib/bridge.mjs';
import { node, withDir } from './helpers.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const script = join(here, '..', 'har-ingest.mjs');

// Fake values only. Each must be absent from every file the bridge writes.
const SECRETS = {
  header: 'hdr-secret-AAAA1111',
  cookie: 'cookie-secret-BBBB2222',
  query: 'query-secret-CCCC3333',
  param: 'passcode-secret-DDDD4444',
  postJson: 'post-secret-EEEE5555',
  response: 'resp-secret-FFFF6666',
  base64: 'b64-secret-GGGG7777',
  body: 'body-secret-HHHH8888',
  bearer: 'bearer-secret-IIII9999',
  capture: 'capture-secret-JJJJ0000',
};
// Non-secret evidence that must survive redaction.
const KEEP = ['offer-42', 'Widget Pro', 'itemId'];

let root;
let session;
let cdpDir;
beforeEach(() => {
  root = mkdtempSync(join(tmpdir(), 'har-ingest-test-'));
  session = join(root, '.octocode', 'octocode-scraping', 'tmp', 'sess');
  cdpDir = join(root, 'cdp-run');
  mkdirSync(session, { recursive: true });
  mkdirSync(cdpDir, { recursive: true });

  const respJson = JSON.stringify({ itemId: 'offer-42', name: 'Widget Pro', access_token: SECRETS.response });
  const har = { log: { entries: [{
    request: {
      method: 'POST',
      url: `https://shop.test/api/login?token=${SECRETS.query}&page=2`,
      headers: [{ name: 'Authorization', value: `Bearer ${SECRETS.header}` }, { name: 'Accept', value: 'application/json' }],
      cookies: [{ name: 'sid', value: SECRETS.cookie }],
      queryString: [{ name: 'token', value: SECRETS.query }, { name: 'page', value: '2' }],
      postData: {
        mimeType: 'application/x-www-form-urlencoded',
        text: `user=dana&passcode=${SECRETS.param}`,
        params: [{ name: 'user', value: 'dana' }, { name: 'passcode', value: SECRETS.param }],
      },
    },
    response: {
      status: 200,
      headers: [{ name: 'Set-Cookie', value: `sid=${SECRETS.cookie}` }],
      cookies: [{ name: 'sid', value: SECRETS.cookie }],
      content: { mimeType: 'application/json', text: respJson },
    },
    time: 12,
  }, {
    request: {
      method: 'POST', url: 'https://shop.test/api/session', headers: [],
      postData: { mimeType: 'application/json', text: JSON.stringify({ itemId: 'offer-42', password: SECRETS.postJson }) },
    },
    response: {
      status: 200, headers: [],
      content: { mimeType: 'application/json', encoding: 'base64', text: Buffer.from(JSON.stringify({ refresh_token: SECRETS.base64, itemId: 'offer-42' })).toString('base64') },
    },
    time: 5,
  }] } };
  writeFileSync(join(cdpDir, 'network.har'), JSON.stringify(har));
  // Short bodies: the old keyword-and-length rule kept anything under 500 characters.
  writeFileSync(join(cdpDir, 'network-bodies.json'), JSON.stringify([
    { requestId: '1', url: 'https://shop.test/api/item', body: JSON.stringify({ itemId: 'offer-42', access_token: SECRETS.body }) },
    { requestId: '2', url: 'https://shop.test/api/me', body: `Authorization: Bearer ${SECRETS.bearer}` },
  ]));
  writeFileSync(join(cdpDir, 'dom-check.json'), JSON.stringify({
    found: true, location: 'https://shop.test/login', role: 'textbox', name: 'Passcode', value: SECRETS.capture, password: SECRETS.capture,
  }));
});
afterEach(() => rmSync(root, { recursive: true, force: true }));

function filesUnder(dir) {
  const out = [];
  for (const name of readdirSync(dir)) {
    const full = join(dir, name);
    if (statSync(full).isDirectory()) out.push(...filesUnder(full));
    else out.push(full);
  }
  return out;
}

function ingest(extra = []) {
  return spawnSync(process.execPath, [script, '--session-dir', session, '--from-cdp-dir', cdpDir, ...extra], { encoding: 'utf8', cwd: root });
}

test('bridge writes no secret value to any session file and keeps the evidence', () => {
  const res = ingest();
  assert.equal(res.status, 0, res.stderr || res.stdout);
  const written = filesUnder(session);
  const corpus = written.map((f) => readFileSync(f, 'utf8')).join('\n');
  for (const [where, value] of Object.entries(SECRETS)) {
    const leaks = written.filter((f) => readFileSync(f, 'utf8').includes(value)).map((f) => f.slice(session.length + 1));
    assert.deepEqual(leaks, [], `${where} secret leaked into ${leaks.join(', ')}`);
  }
  for (const value of KEEP) assert.ok(corpus.includes(value), `lost non-secret evidence: ${value}`);
});

test('bridge does not copy the unredacted bodies source file into the session', () => {
  assert.equal(ingest().status, 0);
  assert.equal(existsSync(join(session, 'cdp', 'source-network-bodies.json')), false);
});

test('--no-redact keeps original values for explicit audits', () => {
  const res = ingest(['--no-redact']);
  assert.equal(res.status, 0, res.stderr || res.stdout);
  const corpus = filesUnder(session).map((f) => readFileSync(f, 'utf8')).join('\n');
  assert.ok(corpus.includes(SECRETS.body));
});

test('redactSecretsInText masks values by key name and token shape, not whole bodies', () => {
  const text = [
    '{"id":"a1","api_key":"k-123","nested":{"pin":4821}}',
    'grant=x&client_secret=s3cr3t&scope=read',
    'Authorization: Bearer abc.def-ghi',
    'jwt eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0.c2lnbmF0dXJlLXZhbHVl here',
    'ghp_0123456789abcdefghijABCDEFGHIJ012345 and AKIAABCDEFGHIJKLMNOP',
  ].join('\n');
  const out = redactSecretsInText(text);
  for (const leaked of ['k-123', '4821', 's3cr3t', 'abc.def-ghi', 'eyJhbGciOiJIUzI1NiJ9', 'ghp_0123456789', 'AKIAABCDEFGHIJKLMNOP']) {
    assert.ok(!out.includes(leaked), `still contains ${leaked}`);
  }
  for (const kept of ['"id":"a1"', 'grant=x', 'scope=read', 'here']) assert.ok(out.includes(kept), `lost ${kept}`);
});

test('redactHarObject keeps large non-secret post bodies intact', () => {
  const big = JSON.stringify({ items: Array.from({ length: 400 }, (_, i) => ({ sku: `sku-${i}` })) });
  const har = { log: { entries: [{ request: { url: 'https://a.test/x', headers: [], postData: { mimeType: 'application/json', text: big } }, response: { headers: [] } }] } };
  redactHarObject(har);
  assert.equal(har.log.entries[0].request.postData.text, big);
});

test('clean parts rejoin byte-exact and in numeric order', async () => withDir(async (dir) => {
  mkdirSync(join(dir, 'text'));
  const parts = Array.from({ length: 11 }, (_, i) => `chunk${i}`);
  parts.forEach((p, i) => writeFileSync(join(dir, 'text', `page-001.clean.part-${String(i + 1).padStart(i === 10 ? 4 : 3, '0')}.md`), p));
  writeFileSync(join(dir, 'text', 'page-001.clean.part-999.md'), 'x');
  writeFileSync(join(dir, 'text', 'page-001.clean.part-1000.md'), 'y');
  const { text } = await concatCleanParts(dir, 'page-001');
  assert.ok(!text.includes('\n'), 'no separator may be inserted between byte-exact parts');
  assert.ok(text.endsWith('xy'), `part-999 must precede part-1000: ${text.slice(-6)}`);
}));

test('har-ingest reruns replace rows, label raw HARs honestly, and accept --min-ms 0', () => withDir((dir) => {
  const harSession = join(dir, 'sess');
  mkdirSync(harSession);
  const har = join(dir, 'net.har');
  writeFileSync(har, JSON.stringify({ log: { entries: [{ request: { method: 'GET', url: 'https://x.test/api', headers: [] }, response: { status: 200, headers: [], content: { mimeType: 'application/json', text: '{}' } }, time: 0 }] } }));
  for (let run = 0; run < 2; run += 1) assert.equal(node('har-ingest.mjs', ['--session-dir', harSession, '--har', har]).status, 0);
  const lines = (rel) => readFileSync(join(harSession, rel), 'utf8').trim().split('\n');
  assert.equal(lines('extracts/cdp-network.jsonl').length, 1, 'rerun duplicated network rows');
  assert.equal(lines('sources.jsonl').length, 1, 'rerun duplicated source rows');
  const raw = node('har-ingest.mjs', ['--session-dir', harSession, '--har', har, '--no-redact', '--filter', 'slow', '--min-ms', '0']);
  assert.equal(raw.status, 0, raw.stdout);
  const out = JSON.parse(raw.stdout);
  assert.ok(out.cdpFiles.some((f) => f.endsWith('.raw.har')), 'an unredacted HAR must not be named .redacted.har');
  assert.equal(out.filteredNetworkRows, 1, '--min-ms 0 must keep a 0 ms entry');
  const handoff = readFileSync(join(harSession, 'extracts/bridge-handoff.json'), 'utf8');
  assert.ok(!handoff.includes('graph-actionability-check'), 'handoff names a script that does not exist');
}));
