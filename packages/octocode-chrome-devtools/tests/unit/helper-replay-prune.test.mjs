// api-replay against a loopback server and prune against a temp artifact folder, through `helper`.
import test from 'node:test';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { existsSync, mkdirSync, utimesSync } from 'node:fs';
import { join } from 'node:path';
import { withHelpers } from './helper-fixture.mjs';

const server = `
let hits = 0;
require('node:http').createServer((req, res) => {
  if (req.url === '/hits') return res.end(String(hits));
  hits++;
  res.setHeader('content-type', 'text/plain');
  res.end(req.method + ':' + 'x'.repeat(2500));
}).listen(0, '127.0.0.1', function () { console.log(this.address().port); });`;

async function loopback() {
  const child = spawn(process.execPath, ['-e', server], {
    stdio: ['ignore', 'pipe', 'inherit'],
  });
  const port = await new Promise((resolve, reject) => {
    child.stdout.once('data', chunk => resolve(String(chunk).trim()));
    child.once('exit', reject);
  });
  const base = `http://127.0.0.1:${port}`;
  return {
    base,
    hits: async () => Number(await (await fetch(`${base}/hits`)).text()),
    stop: () =>
      new Promise(resolve => {
        child.once('exit', resolve);
        child.kill();
      }),
  };
}

test('api-replay sends one GET to loopback, saves the response and pages it without resending', () =>
  withHelpers(async ({ call, exists }) => {
    const web = await loopback();
    try {
      const first = (
        await call('api-replay', [
          '--url',
          `${web.base}/thing`,
          '--max-chars',
          '1000',
        ])
      ).data;
      assert.equal(first.request.method, 'GET');
      assert.equal(first.response.status, 200);
      assert.equal(first.response.bytes, 'GET:'.length + 2500);
      assert.equal(first.hasMore, true);
      assert.equal(first.contentPreview.length, 1000);
      assert.ok(
        exists(first.artifact),
        'the response is saved (workspace-relative)'
      );
      assert.equal(await web.hits(), 1);

      const next = first.next;
      assert.equal(next.tool, 'api-replay');
      const second = (await call('api-replay', next.query.args)).data;
      assert.equal(second.page, 2);
      assert.equal(second.contentPreview.length, 1000);
      assert.equal(await web.hits(), 1, 'the continuation did not resend');
      const refused = await call('api-replay', [
        '--url',
        `${web.base}/x`,
        '--body',
        'data',
      ]);
      assert.ok(refused.error, 'a GET cannot carry a body');
      assert.equal(await web.hits(), 1, 'the refused call sent nothing');
      assert.equal(exists('.octocode'), true);
    } finally {
      await web.stop();
    }
  }));

const BASE = '.octocode/octocode-chrome-devtools/tmp/prune-test';
const runFolder = (cwd, name, ageDays) => {
  const dir = join(cwd, BASE, name);
  mkdirSync(dir, { recursive: true });
  const at = new Date(Date.now() - ageDays * 24 * 3600 * 1000);
  utimesSync(dir, at, at);
};

test('prune --dry-run lists only old run folders; a real run deletes only those', () =>
  withHelpers(async ({ call, cwd }) => {
    const old = ['2020-01-01-00-00-00', '2020-01-02-00-00-00'];
    const fresh = ['2099-01-01-00-00-00', '2099-01-02-00-00-00'];
    for (const name of old) runFolder(cwd, name, 30);
    for (const name of fresh) runFolder(cwd, name, 0);
    mkdirSync(join(cwd, BASE, 'keep-me-not-a-run'), { recursive: true });
    const args = ['--base', BASE, '--max-age-days', '3'];
    const preview = (await call('prune', [...args, '--dry-run'])).data;
    assert.equal(preview.dryRun, true);
    assert.deepEqual(
      preview.runDirs.paths.map(path => path.split('/').pop()).sort(),
      old
    );
    for (const name of [...old, ...fresh])
      assert.ok(existsSync(join(cwd, BASE, name)), `${name} survived dry run`);
    const real = (await call('prune', args)).data;
    assert.equal(real.runDirs.removed, old.length);
    for (const name of old) assert.ok(!existsSync(join(cwd, BASE, name)));
    for (const name of [...fresh, 'keep-me-not-a-run'])
      assert.ok(existsSync(join(cwd, BASE, name)), `${name} was kept`);
  }));

test('prune refuses a base outside the workspace output folder', () =>
  withHelpers(async ({ call }) => {
    const run = await call('prune', ['--base', '/tmp', '--dry-run']);
    assert.ok(run.error);
    assert.match(run.error, /--base must stay under/);
  }));
