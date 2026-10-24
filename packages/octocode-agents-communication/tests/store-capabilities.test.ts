// Store-level guards the black-box CLI tests cannot reach: the Git deadline of activity, activity through a bound store,
// and the rollback of a document publication that fails after linking its file. activity.test.ts, documents.test.ts and
// health.test.ts own the remaining capability behavior.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { existsSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { tempWorkspace } from './helpers.ts';
import type { JsonRecord } from './json-record.ts';
import { fixture, refuses, runtime } from './store-support.ts';

test('a catastrophic pathRegex is cut off at the Git deadline instead of hanging the store', async t => {
  const workspace = tempWorkspace(t, 'store-activity-', { real: true });
  execFileSync('git', ['init', '-q'], { cwd: workspace, stdio: 'pipe' });
  for (let i = 0; i < 3; i++) writeFileSync(join(workspace, `${'a'.repeat(40)}${i}.txt`), 'x');
  const started = performance.now();
  await assert.rejects(runtime.activity.read(workspace, { pathRegex: '^(a+)+$' }), /Git activity exceeded 5 seconds|did not finish|timed out/);
  assert.ok(performance.now() - started < 9000);
});

test('activity runs through the store without joining the workspace to a database', async t => {
  const f = fixture(t, { git: true });
  const id = await f.join('agent');
  writeFileSync(join(f.workspace, 'x.txt'), 'x');
  const result = await f.call(id, 'activity', {});
  assert.ok(result.items.some((item: JsonRecord) => item.path === 'x.txt'));
  await refuses(f.call('nobody', 'activity', {}), /Unknown or expired session/);
});

test('a publication that fails after linking the file leaves no orphan, so the name can be published again', async t => {
  const f = fixture(t);
  const id = await f.join('author');
  const input = { name: 'rollback.md', content: 'published after a failed attempt', reasoning: 'Verify rollback leaves no orphan file' };
  const folder = join(f.workspace, '.octocode/communication');
  const run = f.store.db.run;
  f.store.db.run = function (sql, ...rest) {
    if (sql.startsWith('INSERT INTO records') && rest[0]?.includes('document')) throw new Error('injected append failure');
    return run.call(this, sql, ...rest);
  };
  await assert.rejects(f.call(id, 'share_document', input), /injected append failure/);
  assert.equal(existsSync(join(folder, input.name)), false, 'the rolled-back publication must not leave its file');
  assert.deepEqual(readdirSync(folder).filter(name => name.startsWith('.document-')), []);
  f.store.db.run = run;
  assert.equal((await f.call(id, 'share_document', input)).created, true);
  assert.equal(readFileSync(join(folder, input.name), 'utf8'), input.content);
});
