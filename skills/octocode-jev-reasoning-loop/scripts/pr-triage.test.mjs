import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { runPrTriage } from './pr-triage.mjs';

const options = { repo: 'npm/node-semver', query: 'prerelease', question: 'Fixes prerelease containment?', limit: 4 };
const prs = [11, 22, 33].map(number => ({ number, title: 'Candidate ' + number }));
const nextPage = { tool: 'ghSearchHistory', query: { operation: 'pullRequests', owner: 'npm', repo: 'node-semver', page: 2, keywords: ['prerelease'], pageSize: 4, reasoning: 'Continue discovery.' } };
const discovery = (pullRequests = prs) => ({ results: [{ data: { pullRequests, pagination: { hasMore: true }, next: { nextPage } } }] });
const noScout = () => { throw new Error('Scout must not run.'); };

test('real discovery shape retains all read/gray_read continuations and pagination', () => {
  let calls = 0;
  const result = runPrTriage({ ...options, model: 'jev-1.13.0', output: '/tmp/pr-scout' }, {
    callTool(tool, query) {
      assert.equal(tool, 'ghSearchHistory');
      assert.deepEqual({ ...query, reasoning: '' }, { operation: 'pullRequests', owner: 'npm', repo: 'node-semver', keywords: ['prerelease'], pageSize: 4, reasoning: '' });
      return discovery();
    },
    scout(input, setup) {
      calls++;
      assert.equal(input.taxonomy, 'relevance');
      assert.equal(input.model, 'jev-1.13.0');
      assert.equal(setup.output, '/tmp/pr-scout');
      assert.deepEqual(input.items.map(item => item.id), ['PR11', 'PR22', 'PR33']);
      return { model: input.model, results: {
        PR11: { action: 'read', score: 3 }, PR22: { action: 'gray_read', score: 2 }, PR33: { action: 'skip', score: 0 }
      }, reads: ['PR11'], metrics: { jev: { input_tokens: 10, output_tokens: 3 } } };
    }
  });
  assert.equal(calls, 1);
  assert.deepEqual(result.requiredReads.map(item => item.id), ['PR11', 'PR22']);
  for (const item of result.requiredReads) {
    assert.equal(item.next.tool, 'ghGetHistoryItem');
    assert.deepEqual({ ...item.next.query, reasoning: '' }, {
      operation: 'pullRequest', owner: 'npm', repo: 'node-semver', number: item.number,
      content: { body: true, changedFiles: true }, reasoning: ''
    });
  }
  assert.deepEqual(result.discovery.nextPage, nextPage);
  assert.equal(result.discovery.pagination.hasMore, true);
  assert.match(result.evidenceScope, /never behavior proof or proof of absence/);
  assert.equal(Object.hasOwn(result, 'open_this'), false);
  assert.equal(Object.hasOwn(result, 'next'), false);
});

test('zero and one discovered rows bypass scout while retaining bounded discovery', () => {
  const empty = runPrTriage(options, { callTool: () => discovery([]), scout: noScout });
  assert.equal(empty.status, 'empty');
  assert.deepEqual(empty.requiredReads, []);
  assert.deepEqual(empty.discovery.nextPage, nextPage);
  const single = runPrTriage({ ...options, limit: 1 }, { callTool: () => discovery([prs[0]]), scout: noScout });
  assert.equal(single.status, 'single_candidate');
  assert.equal(single.requiredReads[0].action, 'gray_read');
  assert.equal(single.requiredReads[0].next.query.number, 11);
});

test('dry-run forwards safe scout dry-run and never claims relevance decisions', () => {
  const result = runPrTriage({ ...options, dryRun: true, model: 'jev-1.13.0' }, {
    callTool: () => discovery(),
    scout(input, setup) { assert.equal(setup.dryRun, true); return { request: { model: input.model } }; }
  });
  assert.equal(result.status, 'dry-run');
  assert.equal(result.candidates.length, 3);
  assert.equal(result.request.model, 'jev-1.13.0');
  assert.deepEqual(result.requiredReads, []);
  assert.equal(Object.hasOwn(result, 'ranked'), false);
});

test('discovery errors and malformed rows are errors rather than empty results', () => {
  for (const response of [
    { error: 'network failed' },
    { results: [{ status: 'error', data: { error: 'access denied' } }] },
    { results: [{ data: { errorCode: 'provider.failed' } }] },
    { results: [{ data: {} }] },
    discovery([{ number: '11', title: 'bad number' }]),
    discovery([prs[0], prs[0]])
  ]) assert.throws(() => runPrTriage(options, { callTool: () => response, scout: noScout }));
});

test('missing scout verdict cannot silently drop a required read', () => {
  assert.throws(() => runPrTriage(options, {
    callTool: () => discovery(), scout: () => ({ results: { PR11: { action: 'read' } } })
  }), /no valid verdict/);
});

test('help does not require credentials, discovery or provider access', () => {
  const child = spawnSync(process.execPath, [new URL('./pr-triage.mjs', import.meta.url).pathname, '--help'], { encoding: 'utf8' });
  assert.equal(child.status, 0, child.stderr);
  assert.match(child.stdout, /--model/);
  assert.match(child.stdout, /dry-run still searches GitHub/);
});
