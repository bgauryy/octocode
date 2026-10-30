import { test } from 'node:test';
import type { TestContext } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import type { JsonRecord } from './json-record.ts';
import { execFileSync, binary, tempWorkspace, withReasoning } from './helpers.ts';

/**
 * The reservation rules recorded in test-utils/agent-coordination/contract.json.
 * The vectors cover both the runtime functions and the public lock commands.
 */
const contract = JSON.parse(readFileSync(new URL('../../../test-utils/agent-coordination/contract.json', import.meta.url), 'utf8'));
const { paths, unicode16 } = await import('../dist/runtime.js');

test('contract vectors: components fold, paths key and reservations overlap as the shared contract says', () => {
  assert.deepEqual(contract.fold.map((v: JsonRecord) => unicode16.normalizePathComponent(v.component)), contract.fold.map((v: JsonRecord) => v.folded));
  assert.deepEqual(contract.keys.map((v: JsonRecord) => paths.leaseKey(v.path)), contract.keys.map((v: JsonRecord) => v.key));
  assert.deepEqual(contract.overlap.map((v: JsonRecord) => paths.overlap(v.a.path, v.a.kind, v.b.path, v.b.kind)), contract.overlap.map((v: JsonRecord) => v.overlap));
});

function fixture(t: TestContext) {
  const workspace = tempWorkspace(t, 'communication-contract-', { real: true }), database = join(workspace, 'communication.sqlite');
  const call = (session: string | null, name: string, input: Record<string, unknown> = {}): JsonRecord => JSON.parse(execFileSync(binary, [...name.split(' '), JSON.stringify(withReasoning(name, input)), '--workspace', workspace, '--database', database, ...(session ? ['--session', session] : [])], { encoding: 'utf8', stdio: ['pipe', 'pipe', 'pipe'] }));
  return { call, a: call(null, 'join', { name: 'a', vendor: 'raw' }).id, b: call(null, 'join', { name: 'b', vendor: 'raw' }).id };
}

test('contract vectors: the live lock command refuses exactly the overlapping reservations', t => {
  const f = fixture(t);
  for (const vector of contract.overlap) {
    const held = f.call(f.a, 'lock', vector.a);
    assert.equal(held.ok, true, vector.name);
    const asked = f.call(f.b, 'lock', vector.b);
    assert.equal(asked.ok, !vector.overlap, vector.name);
    f.call(f.a, 'unlock', { leaseId: held.lease.id });
    if (asked.ok) f.call(f.b, 'unlock', { leaseId: asked.lease.id });
  }
});

test('contract vectors: one lock_many call refuses requests that overlap each other and takes nothing', t => {
  const f = fixture(t);
  for (const vector of contract.batches) {
    if (vector.refused) assert.throws(() => f.call(f.a, 'lock_many', { paths: vector.paths }), vector.name);
    else {
      const result = f.call(f.a, 'lock_many', { paths: vector.paths });
      assert.equal(result.ok, true, vector.name);
      for (const lease of result.leases) f.call(f.a, 'unlock', { leaseId: lease.id });
    }
    assert.deepEqual(f.call(f.a, 'locks', { owner: f.a }).items, [], `${vector.name}: nothing stays held`);
  }
});
