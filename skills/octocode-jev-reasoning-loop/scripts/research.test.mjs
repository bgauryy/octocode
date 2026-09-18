import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, writeFileSync, copyFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createHash } from 'node:crypto';
import { checkResearch, narrowClaimToObservedFacts } from './check-research.mjs';

const request = () => ({
  model: 'jev-1.13.0',
  state: {
    claim: 'The current branch rejects oversized input before parsing.',
    evidence: [
      { id: 'E1', source: 'src/check.rs:10-15', scope: 'revision abc', content: 'if oversized { return Err(error); }' },
      { id: 'E2', source: 'src/check.rs:16-20', scope: 'revision abc', content: 'return parse(input);' }
    ],
    evidence_bases: [{ id: 'B1', evidenceIds: ['E1', 'E2'], description: 'Guard precedes the parser call.' }]
  },
  questions: {
    claim_status: { type: 'choice', criteria: { supported: null, contradicted: null, insufficient: null, conflicting: null } },
    decisive_basis: { type: 'choice', criteria: { B1: null, none: null } }
  }
});
const response = (status = 'supported', basis = 'B1') => ({ model: 'jev-1.13.0', answers: {
  claim_status: { type: 'choice', choice: status },
  decisive_basis: { type: 'choice', choice: basis }
} });
const hash = value => createHash('sha256').update(JSON.stringify(value)).digest('hex');
const envelope = (req, res, requestSha256 = hash(req)) => ({ protocol: 'octocode-jev-research/v2', requestSha256, response: res });

test('rejects decisive verdict without a basis, including the observed failure class', () => {
  for (const status of ['supported', 'contradicted']) assert.equal(checkResearch(request(), response(status, 'none')).usable, false);
});
test('accepts a coherent multi-source basis without calling it verified fact', () => {
  for (const status of ['supported', 'contradicted']) {
    const result = checkResearch(request(), response(status));
    assert.equal(result.usable, true);
    assert.equal(result.basisId, 'B1');
    assert.deepEqual(result.evidenceIds, ['E1', 'E2']);
    assert.equal(result.advisoryOnly, true);
  }
  for (const status of ['insufficient', 'conflicting']) {
    assert.equal(checkResearch(request(), response(status, 'none')).usable, true);
    assert.equal(checkResearch(request(), response(status, 'E1')).usable, false);
  }
});

test('claim disagreement returns a concrete deterministic observed-facts narrowing', () => {
  const req = request();
  const result = checkResearch(req, response('insufficient', 'B1'));
  assert.equal(result.usable, false);
  assert.equal(result.reason, 'Claim status and decisive basis disagree.');
  assert.equal(result.suggestion.strategy, 'report_observed_facts');
  assert.deepEqual(result.suggestion.evidenceIds, ['E1', 'E2']);
  assert.match(result.suggestion.claim, /revision abc/);
  assert.match(result.suggestion.claim, /E1 records/);
  assert.equal(result.suggestion.excludedInference, req.state.claim);
  assert.deepEqual(result.suggestion, narrowClaimToObservedFacts(req, response('insufficient', 'B1')));
});
test('rejects invented IDs, invalid statuses, model drift and wrong answer types', () => {
  assert.equal(checkResearch(request(), response('supported', 'invented')).usable, false);
  assert.equal(checkResearch(request(), response('approved')).usable, false);
  const drift = response(); drift.model = 'jev-other';
  assert.equal(checkResearch(request(), drift).usable, false);
  const wrong = response(); wrong.answers.claim_status.type = 'noul';
  assert.equal(checkResearch(request(), wrong).usable, false);
});
test('rejects missing, duplicate, reserved and invalid basis definitions', () => {
  const duplicate = request(); duplicate.state.evidence.push({ ...duplicate.state.evidence[0] });
  assert.equal(checkResearch(duplicate, response()).usable, false);
  const reserved = request(); reserved.state.evidence[0].id = 'none';
  assert.equal(checkResearch(reserved, response()).usable, false);
  const unanchored = request(); delete unanchored.state.evidence[0].source;
  assert.equal(checkResearch(unanchored, response()).usable, false);
  const badBasis = request(); badBasis.state.evidence_bases[0].evidenceIds.push('invented');
  assert.equal(checkResearch(badBasis, response()).usable, false);
  const mismatch = request(); mismatch.questions.decisive_basis.criteria.extra = null;
  assert.equal(checkResearch(mismatch, response()).usable, false);
  for (const value of [null, {}, { state: {} }]) assert.equal(checkResearch(value, response()).usable, false);
  assert.equal(checkResearch(request(), {}).usable, false);
});

test('standalone command requires a response envelope bound to the exact request', () => {
  const dir = mkdtempSync(join(tmpdir(), 'jev-research-'));
  try {
    const script = join(dir, 'check-research.mjs');
    copyFileSync(new URL('./check-research.mjs', import.meta.url), script);
    const req = join(dir, 'request.json'), res = join(dir, 'response.json');
    writeFileSync(req, JSON.stringify(request()));
    writeFileSync(res, JSON.stringify(envelope(request(), response())));
    const run = args => spawnSync(process.execPath, [script, ...args], { cwd: dir, encoding: 'utf8' });
    assert.equal(run(['--help']).status, 0);
    const good = run(['--request', req, '--response', res]);
    assert.equal(good.status, 0);
    assert.equal(JSON.parse(good.stdout).advisoryOnly, true);
    writeFileSync(res, JSON.stringify(envelope(request(), response('supported', 'none'))));
    assert.equal(run(['--request', req, '--response', res]).status, 4);
    writeFileSync(res, JSON.stringify(envelope(request(), response(), '0'.repeat(64))));
    assert.equal(run(['--request', req, '--response', res]).status, 4);
    writeFileSync(res, JSON.stringify(response()));
    assert.equal(run(['--request', req, '--response', res]).status, 4);
    writeFileSync(res, 'invalid JSON with private-value');
    const bad = run(['--request', req, '--response', res]);
    assert.equal(bad.status, 2);
    assert.equal(bad.stderr.includes('private-value'), false);
    assert.equal(run(['--unknown']).status, 2);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('atomic research command exposes its bounded contract', () => {
  const result = spawnSync(process.execPath, [new URL('./research.mjs', import.meta.url).pathname, '--help'], { encoding: 'utf8' });
  assert.equal(result.status, 0);
  assert.match(result.stdout, /Evaluates and checks one bound research request/);
});
