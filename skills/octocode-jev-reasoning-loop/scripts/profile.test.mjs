import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { buildProfileRequests, runProfile } from './profile.mjs';

const aspects = [
  {
    key: 'cohesion',
    type: 'score',
    instructions: 'How cohesive are this source unit responsibilities?',
    criteria: ['Unrelated responsibilities', 'Related responsibilities', 'One coherent responsibility']
  },
  {
    key: 'ownsBehavior',
    type: 'noul',
    instructions: 'Does this source unit directly own runtime behavior?',
    criteria: { true: 'Directly implements behavior', false: 'Only describes, imports, or delegates behavior' }
  },
  {
    key: 'role',
    type: 'choice',
    instructions: 'Which supplied role best describes this source unit?',
    criteria: { implementation: 'Owns behavior', adapter: 'Adapts another owner', data: 'Declares data only' }
  }
];

async function fixture(files) {
  const root = await mkdtemp(join(tmpdir(), 'jev-profile-'));
  for (const [path, content] of Object.entries(files)) await writeFile(join(root, path), content);
  return root;
}

test('buildProfileRequests accepts one path and batches all aspects over its state', async () => {
  const root = await fixture({ 'one.ts': 'export function run() { return 1; }\n' });
  try {
    const built = buildProfileRequests({ root, inputs: [{ id: 'one', path: 'one.ts' }], aspects });
    assert.equal(built.length, 1);
    assert.deepEqual(Object.keys(built[0].request.questions), ['cohesion', 'ownsBehavior', 'role']);
    assert.match(built[0].request.state.source.content, /function run/);
    assert.equal(built[0].source.anchor, 'one.ts:L1-L2');
    assert.equal(built[0].source.coverage, 1);
  } finally { await rm(root, { recursive: true, force: true }); }
});

test('multiple inputs produce independent requests while each request shares its aspects', () => {
  const built = buildProfileRequests({
    inputs: [
      { id: 'alpha', content: 'first unit', source: 'memory:alpha' },
      { id: 'beta', content: 'second unit' }
    ],
    aspects
  });
  assert.equal(built.length, 2);
  assert.equal(built[0].request.state.source.content, 'first unit');
  assert.equal(built[1].request.state.source.content, 'second unit');
  assert.deepEqual(Object.keys(built[0].request.questions), Object.keys(built[1].request.questions));
});

test('profile input is sandboxed, redacted, bounded, and identity-safe', async () => {
  const root = await fixture({ 'secret.txt': 'token=ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZ0123\n' });
  try {
    const built = buildProfileRequests({ root, inputs: [{ id: 'secret', path: 'secret.txt' }], aspects });
    assert.match(built[0].request.state.source.content, /«redacted-token»/);
    assert.throws(() => buildProfileRequests({ root, inputs: [{ id: 'x', path: '../outside' }], aspects }), /sandbox|escape/i);
    assert.throws(() => buildProfileRequests({ inputs: [{ id: 'x', path: 'x', content: 'both' }], aspects }), /exactly one/);
    assert.throws(() => buildProfileRequests({ inputs: [{ id: 'same', content: 'one' }, { id: 'same', content: 'two' }], aspects }), /unique/);
    assert.throws(() => buildProfileRequests({ inputs: [{ id: 'large', content: 'x'.repeat(101) }], aspects, maxChars: 100 }), /exceeds maxChars/);
  } finally { await rm(root, { recursive: true, force: true }); }
});

test('line ranges preserve anchors and report whole-file coverage', async () => {
  const root = await fixture({ 'range.ts': 'one\ntwo\nthree\nfour' });
  try {
    const [built] = buildProfileRequests({ root, inputs: [{ id: 'range', path: 'range.ts', lines: '2-3' }], aspects });
    assert.equal(built.request.state.source.content, 'two\nthree');
    assert.equal(built.source.anchor, 'range.ts:L2-L3');
    assert.ok(built.source.coverage > 0 && built.source.coverage < 1);
  } finally { await rm(root, { recursive: true, force: true }); }
});

test('runProfile dry-run exposes one request per input without contacting Jev', () => {
  const result = runProfile({ inputs: [{ id: 'one', content: 'code' }], aspects }, { dryRun: true });
  assert.equal(result.status, 'dry-run');
  assert.equal(result.requests.length, 1);
  assert.equal(result.requests[0].id, 'one');
  assert.equal(result.requests[0].request.questions.cohesion.type, 'score');
});

test('runProfile evaluates independent inputs concurrently and validates every typed aspect', async () => {
  const dir = await mkdtemp(join(tmpdir(), 'jev-profile-run-'));
  let active = 0;
  let maxActive = 0;
  let calls = 0;
  try {
    const output = await runProfile({
      inputs: [{ id: 'a', content: 'alpha' }, { id: 'b', content: 'beta' }],
      aspects: [
        { key: 'quality', type: 'score', instructions: 'Quality?', criteria: ['low', 'high'] },
        { key: 'flag', type: 'noul', instructions: 'Flag?' },
        { key: 'kind', type: 'choice', instructions: 'Kind?', criteria: { core: 'Core', adapter: 'Adapter' } }
      ]
    }, {
      output: dir,
      evaluate: async request => {
        calls += 1;
        active += 1;
        maxActive = Math.max(maxActive, active);
        await new Promise(resolvePromise => setTimeout(resolvePromise, 15));
        active -= 1;
        return {
          model: request.model,
          answers: {
            quality: { type: 'score', score: 1, legend: { 0: 'low', 1: 'high' } },
            flag: { type: 'noul', noul: 0.75 },
            kind: { type: 'choice', choice: 'core', probabilities: { core: 0.8, adapter: 0.2 } }
          },
          usage: { inputTokens: 1, outputTokens: 1 }
        };
      }
    });
    assert.equal(calls, 2, 'one request per input, not one request per aspect');
    assert.equal(maxActive, 2, 'independent input requests should overlap');
    assert.equal(output.results.length, 2);
    assert.deepEqual(Object.keys(output.results[0].answers), ['quality', 'flag', 'kind']);
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});
