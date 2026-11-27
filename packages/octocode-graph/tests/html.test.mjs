import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, writeFileSync, existsSync } from 'node:fs';
import { gunzipSync } from 'node:zlib';
import { join } from 'node:path';
import { runGraphCli } from '../dist/cli.js';
import { tempDir } from './test-tmp.mjs';

test('standalone HTML embeds complete evidence, worker and assets and publishes only after both inputs are readable', async () => {
  const root = tempDir('graph-html-'),
    path = join(root, 'graph.html');
  const event = '{"seq":0,"type":"run","name":"</script>"}\n';
  const data = JSON.stringify({ files: [{ id: '😀</script>' }] });
  writeFileSync(join(root, 'events.jsonl'), event);
  const run = () => runGraphCli(['html', root, path]);
  await assert.rejects(run());
  assert.equal(existsSync(path), false);
  writeFileSync(join(root, 'data.json'), data);
  assert.equal(await run(), 0);
  const html = readFileSync(path, 'utf8');
  for (const [id, expected] of [
    ['saved-events', event],
    ['saved-data', data],
  ]) {
    const contents = new RegExp(`id="${id}">([^<]+)</script>`).exec(html)[1];
    assert.equal(
      gunzipSync(Buffer.from(contents, 'base64')).toString('utf8'),
      expected
    );
  }
  assert.match(html, /type="importmap"/);
  assert.match(html, /id="saved-worker"/);
  assert.doesNotMatch(html, /<script[^>]+src=/i);
  await assert.rejects(run(), { code: 'EEXIST' });
  assert.equal(readFileSync(path, 'utf8'), html);
});

test('standalone replay rejects a restarted saved log before creating the HTML', async () => {
  const dir = tempDir('graph-mixed-html-'),
    out = join(dir, 'out.html');
  writeFileSync(join(dir, 'data.json'), '{"files":[]}');
  writeFileSync(
    join(dir, 'events.jsonl'),
    [
      { type: 'run', seq: 0, root: '/first' },
      { type: 'run', seq: 0, root: '/second' },
    ]
      .map(e => JSON.stringify(e))
      .join('\n')
  );
  await assert.rejects(runGraphCli(['html', dir, out]), /multiple runs/);
  assert.equal(existsSync(out), false);
});
